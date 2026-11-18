//! Diagnostic proactif : synthèse des échecs récents de tâches, calculée en tâche de fond (ADR 0015).
//!
//! Désactivé par défaut. Une fois activé, le central interroge le fournisseur IA au plus une fois
//! par heure, et seulement si de nouveaux échecs existent. La synthèse reste en mémoire (elle cite
//! des noms de fichiers) ; elle est recalculée après un redémarrage s'il y a encore des échecs.

use super::{admit_owner, converse};
use crate::{
    api::AppState,
    auth::{Principal, PrincipalKind},
    error::{AppError, AppResult},
    repository,
};
use chrono::{SecondsFormat, Utc};
use flux_shared::{
    Role,
    ai::{
        AiChatMessage, AiChatRequest, AiChatRole, AiDiagnosis, AiDiagnosisSettings,
        AiDiagnosisSummary,
    },
};
use sqlx::SqlitePool;
use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

const SETTINGS_KEY: &str = "ai.diagnosis";
/// Délai minimal entre deux appels au fournisseur.
pub const MIN_INTERVAL: Duration = Duration::from_secs(3600);
/// Fréquence à laquelle la boucle regarde s'il y a du nouveau.
pub const TICK: Duration = Duration::from_secs(600);
const WINDOW_HOURS: u32 = 24;
const SYSTEM_OWNER: &str = "system:diagnosis";
const NO_FAILURE_TEXT: &str = "Aucun échec de tâche sur les dernières 24 heures.";
const PROMPT: &str = "Analyse les tâches en échec des dernières 24 heures (outils `list_jobs`, `diagnose_job`, \
    `get_job_stats`). Regroupe-les par cause probable, indique combien de tâches chaque cause concerne \
    et propose pour chacune une action concrète. Reste sous 12 lignes, sans introduction.";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Dernière synthèse et ce qui l'a déclenchée.
#[derive(Clone)]
struct Stored {
    summary: AiDiagnosisSummary,
    last_failure_at: Option<String>,
    generated: Instant,
}

/// Synthèse courante, partagée par la boucle de fond et les routes.
#[derive(Clone, Default)]
pub struct DiagnosisStore {
    inner: Arc<Mutex<Option<Stored>>>,
}

impl DiagnosisStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn put(&self, stored: Stored) {
        *lock(&self.inner) = Some(stored);
    }

    fn current(&self) -> Option<Stored> {
        lock(&self.inner).clone()
    }

    /// Oublie la synthèse (tests : repartir d'un état « jamais calculé »).
    #[cfg(test)]
    pub fn clear_for_tests(&self) {
        *lock(&self.inner) = None;
    }

    pub fn summary(&self) -> Option<AiDiagnosisSummary> {
        self.current().map(|stored| stored.summary)
    }
}

pub async fn is_enabled(pool: &SqlitePool) -> bool {
    repository::get_setting(pool, SETTINGS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<AiDiagnosisSettings>(&raw).ok())
        .is_some_and(|settings| settings.enabled)
}

pub async fn set_enabled(pool: &SqlitePool, enabled: bool) -> AppResult<()> {
    let encoded =
        serde_json::to_string(&AiDiagnosisSettings { enabled }).map_err(|_| AppError::Internal)?;
    repository::set_setting(pool, SETTINGS_KEY, &encoded).await
}

pub async fn current(state: &AppState) -> AiDiagnosis {
    AiDiagnosis {
        enabled: is_enabled(&state.pool).await,
        summary: state.ai_diagnosis.summary(),
    }
}

/// Échecs de la fenêtre analysée : leur nombre et l'instant du dernier.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Failures {
    count: u32,
    latest: Option<String>,
}

async fn recent_failures(pool: &SqlitePool) -> AppResult<Failures> {
    let (count, latest): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(updated_at) FROM jobs
         WHERE status = 'failed' AND updated_at > datetime('now', ?1)",
    )
    .bind(format!("-{WINDOW_HOURS} hours"))
    .fetch_one(pool)
    .await?;
    Ok(Failures {
        count: u32::try_from(count).unwrap_or(u32::MAX),
        latest,
    })
}

/// Faut-il (re)calculer la synthèse ? Seulement s'il y a des échecs, que le dernier calcul date
/// d'au moins `MIN_INTERVAL` et que les échecs ont changé depuis.
fn is_due(previous: Option<&Stored>, failures: &Failures, now: Instant) -> bool {
    if failures.count == 0 {
        return false;
    }
    let Some(previous) = previous else {
        return true;
    };
    now.duration_since(previous.generated) >= MIN_INTERVAL
        && (previous.summary.failures != failures.count
            || previous.last_failure_at != failures.latest)
}

fn system_principal() -> Principal {
    Principal {
        kind: PrincipalKind::ApiKey,
        id: SYSTEM_OWNER.to_owned(),
        name: "Diagnostic proactif".to_owned(),
        role: Role::Admin,
        scopes: None,
        session_hash: None,
    }
}

/// Calcule et garde la synthèse ; sans échec, aucun appel au fournisseur.
async fn generate(state: &AppState, failures: Failures) -> AppResult<AiDiagnosisSummary> {
    let text = if failures.count == 0 {
        NO_FAILURE_TEXT.to_owned()
    } else {
        admit_owner(state, SYSTEM_OWNER).await?;
        let request = AiChatRequest {
            messages: vec![AiChatMessage {
                role: AiChatRole::User,
                content: format!("{PROMPT} Il y en a {} dans cette fenêtre.", failures.count),
            }],
            context_hint: None,
            context: None,
            conversation_id: None,
        };
        // Sans run, seuls les outils en lecture sont proposés au modèle.
        converse(state, &system_principal(), request, None)
            .await?
            .reply
    };
    let summary = AiDiagnosisSummary {
        text,
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        failures: failures.count,
    };
    state.ai_diagnosis.put(Stored {
        summary: summary.clone(),
        last_failure_at: failures.latest,
        generated: Instant::now(),
    });
    Ok(summary)
}

/// Recalcul demandé par un administrateur ; `Conflict` si le diagnostic proactif est coupé.
pub async fn refresh(state: &AppState) -> AppResult<AiDiagnosis> {
    if !is_enabled(&state.pool).await {
        return Err(AppError::Conflict);
    }
    let failures = recent_failures(&state.pool).await?;
    generate(state, failures).await?;
    Ok(current(state).await)
}

/// Un passage de la boucle : recalcule si c'est activé, utile et dû.
pub async fn tick(state: &AppState) -> AppResult<()> {
    if !is_enabled(&state.pool).await {
        return Ok(());
    }
    let failures = recent_failures(&state.pool).await?;
    if is_due(
        state.ai_diagnosis.current().as_ref(),
        &failures,
        Instant::now(),
    ) {
        generate(state, failures).await?;
    }
    Ok(())
}

/// Boucle de fond du central.
pub async fn run_loop(state: AppState) {
    loop {
        tokio::time::sleep(TICK).await;
        if let Err(error) = tick(&state).await {
            tracing::debug!(%error, "diagnostic proactif non calculé");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(failures: u32, latest: &str, age: Duration) -> Stored {
        Stored {
            summary: AiDiagnosisSummary {
                text: "x".into(),
                generated_at: "2026-11-18T10:00:00Z".into(),
                failures,
            },
            last_failure_at: Some(latest.into()),
            generated: Instant::now().checked_sub(age).unwrap_or_else(Instant::now),
        }
    }

    fn failures(count: u32, latest: &str) -> Failures {
        Failures {
            count,
            latest: Some(latest.into()),
        }
    }

    #[test]
    fn nothing_is_computed_without_failures_and_the_first_failure_triggers_it() {
        let now = Instant::now();
        assert!(!is_due(
            None,
            &Failures {
                count: 0,
                latest: None
            },
            now
        ));
        assert!(is_due(None, &failures(1, "a"), now));
    }

    #[test]
    fn a_recent_summary_waits_and_an_unchanged_one_is_not_recomputed() {
        let now = Instant::now();
        let fresh = stored(2, "a", Duration::from_secs(60));
        assert!(!is_due(Some(&fresh), &failures(5, "b"), now));
        let old_same = stored(2, "a", MIN_INTERVAL + Duration::from_secs(1));
        assert!(!is_due(Some(&old_same), &failures(2, "a"), now));
        assert!(is_due(Some(&old_same), &failures(3, "a"), now));
        assert!(is_due(Some(&old_same), &failures(2, "b"), now));
    }
}
