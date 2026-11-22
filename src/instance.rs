//! Entrée du processus et sélection de son rôle au démarrage.
//!
//! Sans argument, le rôle est celui conservé dans `/config` : un worker enrôlé reprend son rôle, tout
//! autre installation démarre en central (et propose l'assistant d'installation). `central` et `worker`
//! restent acceptés comme arguments ; `worker` sans enrôlement sert la page d'enrôlement (ADR 0016).

use crate::{
    account_admin, api, config,
    config::Config,
    repository, secret_rotation, secrets,
    worker::{self, WorkerConfig},
    worker_settings,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Central,
    /// Worker demandé mais pas encore enrôlé : seule la page d'enrôlement est servie.
    WorkerSetup,
    Worker,
}

/// Dossier de travail de la page d'enrôlement, sous `/config`.
const WORKER_SETUP_DIR: &str = "worker-setup";

/// L'installation vierge est réservée à un worker (commande `worker`).
static WORKER_SETUP_ONLY: AtomicBool = AtomicBool::new(false);
static ROLE_SWITCH_REQUESTED: AtomicBool = AtomicBool::new(false);
static ROLE_SWITCH_NOTIFY: Notify = Notify::const_new();

/// Réserve cette installation vierge à l'enrôlement d'un worker.
fn reserve_for_worker() {
    WORKER_SETUP_ONLY.store(true, Ordering::SeqCst);
}

/// `true` si cette installation vierge ne peut devenir qu'un worker.
pub fn worker_setup_only() -> bool {
    WORKER_SETUP_ONLY.load(Ordering::SeqCst)
}

/// Demande l'arrêt propre du central pour que le processus reparte avec le rôle enregistré
/// (appelé une fois l'enrôlement en worker terminé).
pub fn request_role_switch() {
    ROLE_SWITCH_REQUESTED.store(true, Ordering::SeqCst);
    ROLE_SWITCH_NOTIFY.notify_waiters();
}

/// Se termine quand un changement de rôle est demandé.
pub async fn role_switch_requested() {
    loop {
        let notified = ROLE_SWITCH_NOTIFY.notified();
        if ROLE_SWITCH_REQUESTED.load(Ordering::SeqCst) {
            return;
        }
        notified.await;
    }
}

/// Consomme la demande de changement de rôle : `true` si elle était en attente.
fn take_role_switch() -> bool {
    ROLE_SWITCH_REQUESTED.swap(false, Ordering::SeqCst)
}

fn parse_role(argument: Option<&str>) -> anyhow::Result<Option<Role>> {
    match argument {
        Some("worker") => Ok(Some(Role::WorkerSetup)),
        Some("central") => Ok(Some(Role::Central)),
        None => Ok(None),
        _ => anyhow::bail!(
            "Usage: forgearr [central|worker|reset-password <identifiant>|rotate-secret|--version]"
        ),
    }
}

/// Rôle de départ : `central` s'impose, sinon le rôle conservé dans `config_dir` décide.
fn initial_role(argument: Option<&str>, config_dir: &Path) -> anyhow::Result<Role> {
    let requested = parse_role(argument)?;
    if requested == Some(Role::Central) {
        return Ok(Role::Central);
    }
    Ok(if worker_settings::load_worker(config_dir)?.is_some() {
        Role::Worker
    } else {
        requested.unwrap_or(Role::Central)
    })
}

/// Signale un problème de démarrage dans les journaux et sur la sortie du conteneur.
fn warn_operator(message: &str) {
    tracing::warn!("{message}");
    eprintln!("{message}");
}

/// Prévient qu'une ancienne clé fournie par l'environnement est ignorée (sa valeur n'est jamais journalisée).
fn warn_retired_variables() {
    let retired = worker_settings::retired_key_variables(|name| std::env::var(name).ok());
    if !retired.is_empty() {
        warn_operator(&format!(
            "{} ignoré : un worker ne reçoit plus de clé par l'environnement. Ouvrez sa page (port 3000) pour l'enrôler, sous le même nom s'il est déjà connu du central, puis retirez ces variables.",
            retired.join(", ")
        ));
    }
}

/// Prépare un worker sans enrôlement : `/config` inscriptible et pas de central installé à cet endroit.
/// Les dossiers de travail passent sous `/config` : les médias montés sur `/data` peuvent être en lecture seule.
pub async fn prepare_worker_setup(config: &mut Config) -> anyhow::Result<()> {
    worker_settings::ensure_writable(&config.config_dir)?;
    let pool = api::connect_database(config).await?;
    let accounts = repository::account_count(&pool).await;
    pool.close().await;
    anyhow::ensure!(
        accounts? == 0,
        "{} contient un central installé : donnez au worker son propre dossier de configuration, ou retirez la commande « worker »",
        config.config_dir.display()
    );
    reserve_for_worker();
    config.local_worker = false;
    config.data_dir = config.config_dir.join(WORKER_SETUP_DIR);
    config.input_dir = config.data_dir.join("input");
    Ok(())
}

/// Réinitialise le mot de passe d'un compte de ce central et l'affiche une seule fois.
async fn reset_password(username: Option<&str>) -> anyhow::Result<()> {
    let username =
        username.ok_or_else(|| anyhow::anyhow!("Usage: forgearr reset-password <identifiant>"))?;
    let pool = api::connect_database(&Config::load()?).await?;
    let new_password = account_admin::reset_password(&pool, username)
        .await
        .map_err(|error| anyhow::anyhow!("réinitialisation impossible : {error}"))?;
    println!("Nouveau mot de passe de {username} : {new_password}");
    println!("Il n'est affiché qu'une fois ; les sessions du compte sont fermées.");
    Ok(())
}

/// Fait tourner la clé de chiffrement locale et rechiffre tous les secrets, central arrêté.
async fn rotate_secret() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var(secrets::KEY_FILE_ENV).is_err(),
        "{} est défini : la clé appartient à l'orchestrateur, remplacez ce fichier vous-même",
        secrets::KEY_FILE_ENV
    );
    let config = Config::load()?;
    let pool = api::connect_database(&config).await?;
    let outcome = secret_rotation::rotate_local_key(&pool, &config.config_dir)
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "rechiffrement impossible : les anciennes clés restent dans le trousseau, aucun secret n'est perdu ({error})"
            )
        })?;
    println!(
        "Clé {} remplacée par {} : {} valeur(s) de plugins et {} en-tête(s) de webhook rechiffrés.",
        outcome.switch.old_key_id,
        outcome.switch.new_key_id,
        outcome.report.plugin_values,
        outcome.report.flow_values
    );
    Ok(())
}

pub async fn run(argument: Option<&str>, operand: Option<&str>) -> anyhow::Result<()> {
    if argument == Some("--version") {
        println!("{}", crate::version::app_version());
        return Ok(());
    }
    if argument == Some("rotate-secret") {
        return rotate_secret().await;
    }
    if argument == Some("reset-password") {
        return reset_password(operand).await;
    }
    let config_dir = config::resolve_config_dir();
    let mut role = initial_role(argument, &config_dir)?;
    if role != Role::Worker {
        warn_retired_variables();
    }
    loop {
        match role {
            Role::Worker => {
                return worker::run(WorkerConfig::load(&config_dir)?).await;
            }
            Role::Central | Role::WorkerSetup => {
                let mut config = Config::load()?;
                if role == Role::WorkerSetup {
                    prepare_worker_setup(&mut config).await?;
                } else if let Err(error) = worker_settings::ensure_writable(&config_dir) {
                    warn_operator(&format!("{error:#}"));
                }
                api::serve(config).await?;
                if !take_role_switch() {
                    return Ok(());
                }
                tracing::info!("changement de rôle : redémarrage en worker");
                role = initial_role(None, &config_dir)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arguments_select_the_role() -> anyhow::Result<()> {
        assert_eq!(parse_role(Some("worker"))?, Some(Role::WorkerSetup));
        assert_eq!(parse_role(Some("central"))?, Some(Role::Central));
        assert_eq!(parse_role(None)?, None);
        assert!(parse_role(Some("autre")).is_err());
        Ok(())
    }

    #[test]
    fn test_worker_command_waits_for_the_enrollment() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        assert_eq!(initial_role(Some("worker"), dir.path())?, Role::WorkerSetup);
        assert_eq!(initial_role(None, dir.path())?, Role::Central);
        assert!(WorkerConfig::load(dir.path()).is_err());

        let settings = worker_settings::WorkerSettings {
            name: "nas-1".into(),
            central_url: "http://central:3000".into(),
        };
        worker_settings::save_worker(dir.path(), &settings, "fdf_secret")?;
        assert_eq!(initial_role(Some("worker"), dir.path())?, Role::Worker);
        assert_eq!(initial_role(None, dir.path())?, Role::Worker);
        assert_eq!(initial_role(Some("central"), dir.path())?, Role::Central);

        let loaded = WorkerConfig::load(dir.path())?;
        assert_eq!(loaded.name, settings.name);
        assert_eq!(loaded.key, "fdf_secret");
        assert_eq!(loaded.url.as_str(), "http://central:3000/");
        Ok(())
    }

    #[tokio::test]
    async fn test_role_switch_wakes_waiters_once_requested() {
        let waiter = tokio::spawn(role_switch_requested());
        tokio::task::yield_now().await;
        request_role_switch();
        assert!(waiter.await.is_ok());
        assert!(take_role_switch());
        assert!(!take_role_switch());
    }
}
