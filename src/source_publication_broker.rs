//! Coordination des publications de résultats sur les détenteurs distants.

use crate::error::{AppError, AppResult};
use forgearr_shared::transfer::{
    SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Notify, oneshot};
use uuid::Uuid;

struct Pending {
    task: SourcePublicationTask,
    staged: PathBuf,
    dispatched: bool,
    result: oneshot::Sender<SourcePublicationOutcome>,
}

#[derive(Default)]
struct Inner {
    pending: Mutex<HashMap<Uuid, Pending>>,
    notify: Notify,
}

#[derive(Clone, Default)]
pub struct SourcePublicationBroker {
    inner: Arc<Inner>,
}

impl SourcePublicationBroker {
    /// Enregistre la demande et attend la réponse du détenteur pendant une durée bornée.
    pub async fn request(
        &self,
        task: SourcePublicationTask,
        staged: PathBuf,
        timeout: Duration,
    ) -> AppResult<SourcePublicationOutcome> {
        let id = task.request_id;
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.inner.pending.lock().await;
            if pending.contains_key(&id) {
                return Err(AppError::Conflict);
            }
            pending.insert(
                id,
                Pending {
                    task,
                    staged,
                    dispatched: false,
                    result: tx,
                },
            );
        }
        self.inner.notify.notify_waiters();
        let outcome = tokio::time::timeout(timeout, rx).await;
        self.inner.pending.lock().await.remove(&id);
        outcome
            .ok()
            .and_then(Result::ok)
            .ok_or(AppError::CentralUnavailable)
    }

    /// Ne distribue chaque demande qu'au détenteur désigné.
    pub async fn take_for_worker(
        &self,
        worker: Uuid,
        wait: Duration,
    ) -> Vec<SourcePublicationTask> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let notified = self.inner.notify.notified();
            let tasks = {
                let mut pending = self.inner.pending.lock().await;
                pending
                    .values_mut()
                    .filter(|p| p.task.worker_id == worker && !p.dispatched)
                    .map(|p| {
                        p.dispatched = true;
                        p.task.clone()
                    })
                    .collect::<Vec<_>>()
            };
            if !tasks.is_empty() || wait.is_zero() {
                return tasks;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Vec::new();
            }
        }
    }

    /// Rend le fichier temporaire uniquement au détenteur auquel la demande a été remise.
    pub async fn staged_for_worker(&self, request: Uuid, worker: Uuid) -> Option<PathBuf> {
        self.inner
            .pending
            .lock()
            .await
            .get(&request)
            .filter(|p| p.dispatched && p.task.worker_id == worker)
            .map(|p| p.staged.clone())
    }

    /// Accepte une seule réponse, du détenteur auquel la demande a été remise.
    pub async fn report(&self, report: SourcePublicationReport) -> bool {
        let mut pending = self.inner.pending.lock().await;
        if !pending
            .get(&report.request_id)
            .is_some_and(|p| p.dispatched && p.task.worker_id == report.worker_id)
        {
            return false;
        }
        pending
            .remove(&report.request_id)
            .is_some_and(|p| p.result.send(report.outcome).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::libraries::LibraryOutputPolicy;

    #[tokio::test]
    async fn only_the_dispatched_holder_can_read_and_report() -> anyhow::Result<()> {
        let broker = SourcePublicationBroker::default();
        let holder = Uuid::new_v4();
        let task = SourcePublicationTask {
            request_id: Uuid::new_v4(),
            worker_id: holder,
            root: "/media".into(),
            source_name: "a.mkv".into(),
            source_sha256: "a".repeat(64),
            source_bytes: 10,
            result_sha256: "b".repeat(64),
            result_bytes: 9,
            extension: "mkv".into(),
            policy: LibraryOutputPolicy::ReplaceWithBackup,
        };
        let request_id = task.request_id;
        let requester = broker.clone();
        let waiting = tokio::spawn(async move {
            requester
                .request(task, PathBuf::from("/staged"), Duration::from_secs(5))
                .await
        });
        let tasks = broker.take_for_worker(holder, Duration::from_secs(5)).await;
        assert_eq!(tasks.len(), 1);
        assert!(
            broker
                .staged_for_worker(request_id, Uuid::new_v4())
                .await
                .is_none()
        );
        assert_eq!(
            broker.staged_for_worker(request_id, holder).await,
            Some(PathBuf::from("/staged"))
        );
        let report = SourcePublicationReport {
            request_id,
            worker_id: holder,
            outcome: SourcePublicationOutcome::SourceChanged,
        };
        assert!(
            !broker
                .report(SourcePublicationReport {
                    worker_id: Uuid::new_v4(),
                    ..report.clone()
                })
                .await
        );
        assert!(broker.report(report.clone()).await);
        assert!(!broker.report(report).await);
        assert_eq!(waiting.await??, SourcePublicationOutcome::SourceChanged);
        Ok(())
    }
}
