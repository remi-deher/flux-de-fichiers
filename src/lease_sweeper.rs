//! Balayage continu des baux (ADR 0027) : tâches injoignables et échéances, indépendamment des
//! demandes de tâche des workers.

use crate::{
    api::{self, AppState},
    repository,
};
use chrono::{DateTime, Utc};
use std::time::Duration;

/// Intervalle entre deux passages.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Balaye jusqu'à l'arrêt du central ; une panique d'un passage n'arrête pas la boucle.
pub async fn run(state: AppState) {
    let mut previous = Some(Utc::now());
    loop {
        tokio::select! {
            _ = tokio::time::sleep(SWEEP_INTERVAL) => {},
            _ = api::shutdown_signal() => return,
        }
        let pass = tokio::spawn(sweep_once(state.clone(), previous));
        if let Err(error) = pass.await {
            tracing::error!(%error, "passage du balayage des baux interrompu");
        }
        // Fin du passage, même en erreur : l'attente d'un verrou n'est pas une suspension.
        previous = Some(Utc::now());
    }
}

/// Un passage : balaye puis notifie les changements.
async fn sweep_once(state: AppState, previous: Option<DateTime<Utc>>) {
    let in_flight = state.publications.keys();
    match repository::sweep(&state.pool, previous, &in_flight).await {
        Ok(pass) => api::leases::notify_lease_changes(&state, &pass.changes),
        Err(error) => tracing::warn!(%error, "balayage des baux impossible"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::transfers::tests::setup,
        domain::{JobErrorCode, JobEventType},
    };

    const EVENT_LIMIT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn a_pass_notifies_the_end_of_an_overdue_lease() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, _, _) = setup(dir.path()).await?;
        sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 second') WHERE id=?1")
            .bind(job.id)
            .execute(&state.pool)
            .await?;
        let mut events = state.event_history.subscribe_from(None).await.rx;
        sweep_once(state.clone(), None).await;
        let (_, event) = tokio::time::timeout(EVENT_LIMIT, events.recv()).await??;
        let crate::live::LiveEvent::Job(event) = event else {
            anyhow::bail!("événement de tâche attendu");
        };
        assert_eq!(event.job_id, job.id);
        assert_eq!(event.event_type, JobEventType::JobFailed);
        assert_eq!(event.error_code, Some(JobErrorCode::LeaseExpired));
        Ok(())
    }
}
