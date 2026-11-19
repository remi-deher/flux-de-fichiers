//! Rotation de la clé de chiffrement des secrets (ADR 0011), possible central en marche.
//!
//! 1. Bascule : l'ancienne clé rejoint le trousseau des clés retirées, puis une clé neuve devient
//!    active. Les nouvelles écritures l'utilisent aussitôt ; les anciennes valeurs restent lisibles.
//! 2. Rechiffrement : tous les secrets stockés passent sous la clé active en une seule transaction ;
//!    si une valeur ne se déchiffre pas, rien ne change et les clés retirées sont gardées.
//! 3. Retrait : une fois la transaction validée, plus aucune valeur n'utilise de clé retirée, qui est
//!    oubliée. Aucune étape ne détruit une clé encore utile : il n'y a rien à sauvegarder en clair.

use crate::{
    error::{AppError, AppResult},
    flows, plugins, repository,
    secrets::{self, SecretBox},
    worker_settings,
};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Duration,
};

/// Délai laissé, central en marche, aux requêtes qui ont ouvert l'ancienne clé avant la bascule pour
/// terminer leur écriture, avant le rechiffrement et le retrait.
pub const RETIRED_KEY_GRACE: Duration = Duration::from_secs(30);

/// Une seule rotation à la fois par trousseau (dossier de configuration) : une seconde bascule ou une
/// restauration pendant le délai de grâce est refusée.
static ROTATIONS_IN_PROGRESS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Valeurs rechiffrées, par origine.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RotationReport {
    pub plugin_values: usize,
    pub flow_values: usize,
}

/// Identifiants des clés avant et après la bascule.
#[derive(Debug, PartialEq, Eq)]
pub struct KeySwitch {
    pub old_key_id: String,
    pub new_key_id: String,
}

#[derive(Debug)]
pub struct RotationOutcome {
    pub switch: KeySwitch,
    pub report: RotationReport,
}

/// Rechiffre toutes les valeurs stockées de `old` vers `new`, dans une seule transaction.
pub async fn rotate_values(
    pool: &SqlitePool,
    old: &SecretBox,
    new: &SecretBox,
) -> AppResult<RotationReport> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut report = RotationReport::default();
    let settings =
        sqlx::query!(r#"SELECT plugin_id AS "plugin_id!: String", settings FROM plugin_settings"#)
            .fetch_all(&mut *tx)
            .await?;
    for row in settings {
        if let Some((rewritten, count)) =
            plugins::rotate_settings(old, new, &row.plugin_id, &row.settings)?
        {
            sqlx::query!(
                "UPDATE plugin_settings SET settings = ?1 WHERE plugin_id = ?2",
                rewritten,
                row.plugin_id
            )
            .execute(&mut *tx)
            .await?;
            report.plugin_values += count;
        }
    }
    for store in repository::DEFINITION_STORES {
        let definitions = store.fetch_definitions(&mut *tx).await?;
        for (flow_id, rowid, definition) in definitions {
            if let Some((rewritten, count)) =
                flows::rotate_definition(old, new, &flow_id, &definition)?
            {
                store.update_definition(&mut *tx, rowid, &rewritten).await?;
                report.flow_values += count;
            }
        }
    }
    report.plugin_values +=
        crate::runtime_settings::rotate_secrets_in_tx(&mut tx, old, new).await?;
    report.plugin_values += crate::ai::settings::rotate_secrets_in_tx(&mut tx, old, new).await?;
    report.plugin_values += crate::ai::history::rotate_secrets_in_tx(&mut tx, old, new).await?;
    tx.commit().await?;
    Ok(report)
}

fn storage_error(error: anyhow::Error) -> AppError {
    tracing::error!(error = %error, "écriture de la clé de chiffrement impossible");
    AppError::Internal
}

/// Étape 1 : retire la clé locale active, puis en publie une neuve. Refusée si la clé vient de
/// `SECRET_KEY_FILE`, car elle appartient alors à l'orchestrateur.
pub fn switch_active_key(
    config_dir: &Path,
    external_key_file: Option<&Path>,
) -> AppResult<KeySwitch> {
    if external_key_file.is_some() {
        return Err(AppError::Conflict);
    }
    let old = SecretBox::open_with(config_dir, None)?;
    let key_path: PathBuf = SecretBox::local_key_path(config_dir);
    secrets::retire_key(config_dir, &std::fs::read_to_string(&key_path)?)?;
    let new_hex = SecretBox::generate_key_hex();
    let new = SecretBox::from_key_hex(&new_hex)?;
    worker_settings::write_private(&key_path, &new_hex).map_err(storage_error)?;
    Ok(KeySwitch {
        old_key_id: old.key_id().to_owned(),
        new_key_id: new.key_id().to_owned(),
    })
}

/// Étapes 2 et 3 : rechiffre tout sous la clé active, puis oublie les clés retirées. Sans clé retirée,
/// rien n'est rechiffré. Sert aussi au démarrage, pour achever une rotation interrompue.
pub async fn reseal_and_retire(
    pool: &SqlitePool,
    config_dir: &Path,
    external_key_file: Option<&Path>,
) -> AppResult<RotationReport> {
    let ring = SecretBox::open_with(config_dir, external_key_file)?;
    if ring.retired_key_ids().is_empty() {
        // Le fichier peut encore lister la clé active (kit restauré sur sa propre clé) : rien n'en dépend.
        secrets::clear_retired_keys(config_dir)?;
        return Ok(RotationReport::default());
    }
    let report = rotate_values(pool, &ring, &ring).await?;
    secrets::clear_retired_keys(config_dir)?;
    Ok(report)
}

/// Rotation complète d'un coup, central arrêté (`flux-de-fichiers rotate-secret`).
pub async fn rotate_local_key(pool: &SqlitePool, config_dir: &Path) -> AppResult<RotationOutcome> {
    let switch = switch_active_key(config_dir, None)?;
    let report = reseal_and_retire(pool, config_dir, None).await?;
    Ok(RotationOutcome { switch, report })
}

/// Verrou de rotation d'un trousseau, libéré quand la tâche se termine, même en cas d'échec.
struct RotationGuard {
    config_dir: PathBuf,
}

impl RotationGuard {
    fn acquire(config_dir: &Path) -> AppResult<Self> {
        let mut running = ROTATIONS_IN_PROGRESS
            .lock()
            .map_err(|_| AppError::Internal)?;
        if !running.insert(config_dir.to_path_buf()) {
            return Err(AppError::Conflict);
        }
        Ok(Self {
            config_dir: config_dir.to_path_buf(),
        })
    }
}

impl Drop for RotationGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = ROTATIONS_IN_PROGRESS.lock() {
            running.remove(&self.config_dir);
        }
    }
}

/// Restauration depuis un kit : ses clés rejoignent le trousseau des clés retirées, puis tout est
/// rechiffré sous la clé active. Si une valeur reste illisible, les clés du kit sont gardées.
pub async fn restore_keys(
    pool: &SqlitePool,
    config_dir: &Path,
    external_key_file: Option<&Path>,
    keys_hex: &[String],
) -> AppResult<RotationReport> {
    let _guard = RotationGuard::acquire(config_dir)?;
    for key_hex in keys_hex {
        secrets::retire_key(config_dir, key_hex)?;
    }
    reseal_and_retire(pool, config_dir, external_key_file).await
}

/// Rotation central en marche : bascule tout de suite, puis rechiffre et retire en tâche de fond
/// après `grace`. Refusée si une rotation est déjà en cours.
pub fn start_online_rotation(
    pool: SqlitePool,
    config_dir: PathBuf,
    external_key_file: Option<PathBuf>,
    grace: Duration,
) -> AppResult<KeySwitch> {
    let guard = RotationGuard::acquire(&config_dir)?;
    let switch = switch_active_key(&config_dir, external_key_file.as_deref())?;
    tokio::spawn(async move {
        let _guard = guard;
        tokio::time::sleep(grace).await;
        match reseal_and_retire(&pool, &config_dir, None).await {
            Ok(report) => tracing::info!(
                plugin_values = report.plugin_values,
                flow_values = report.flow_values,
                "secrets rechiffrés sous la nouvelle clé, anciennes clés retirées"
            ),
            Err(error) => tracing::error!(
                error = %error,
                "rechiffrement impossible : les anciennes clés sont gardées pour déchiffrer"
            ),
        }
    });
    Ok(switch)
}
