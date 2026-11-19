//! Repli par le central (ADR 0019) : relais en flux, sans stockage, d'une source détenue par un
//! worker vers le worker traitant, quand aucune route directe n'est utilisable.

use crate::error::{AppError, AppResult};
use axum::body::Bytes;
use forgearr_shared::transfer::SourceRelayTask;
use std::{collections::HashMap, io, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Notify, mpsc, oneshot};
use uuid::Uuid;

/// Attente maximale qu'un worker source commence à envoyer le fichier.
pub const SOURCE_RELAY_START_TIMEOUT: Duration = Duration::from_secs(20);
/// Morceaux en vol entre le worker source et le worker traitant (contre-pression).
const RELAY_BUFFERED_CHUNKS: usize = 8;

/// Flux reçu par le worker traitant.
pub type RelayChunk = Result<Bytes, io::Error>;

struct Pending {
    task: SourceRelayTask,
    dispatched: bool,
    expires_at: tokio::time::Instant,
    started: Option<oneshot::Sender<()>>,
    sink: mpsc::Sender<RelayChunk>,
}

#[derive(Default)]
struct Inner {
    state: Mutex<HashMap<Uuid, Pending>>,
    notify: Notify,
}

/// Courtier en mémoire : demandes en attente par worker source, comme le courtier d'exploration.
#[derive(Clone, Default)]
pub struct SourceRelayBroker {
    inner: Arc<Inner>,
}

/// Ce que le worker source reçoit pour écrire dans le flux d'un transfert accepté.
pub struct RelaySink {
    sink: mpsc::Sender<RelayChunk>,
}

impl RelaySink {
    /// `false` si le worker traitant a abandonné : le worker source peut arrêter.
    pub async fn send(&self, chunk: RelayChunk) -> bool {
        self.sink.send(chunk).await.is_ok()
    }
}

impl SourceRelayBroker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Demande à `worker_id` d'envoyer la source ; rend le flux dès que l'envoi a commencé.
    pub async fn request(
        &self,
        worker_id: Uuid,
        root: String,
        source_name: String,
        timeout: Duration,
    ) -> AppResult<mpsc::Receiver<RelayChunk>> {
        let (sink, stream) = mpsc::channel(RELAY_BUFFERED_CHUNKS);
        let (started_tx, started_rx) = oneshot::channel();
        let request_id = Uuid::new_v4();
        {
            let mut state = self.inner.state.lock().await;
            let now = tokio::time::Instant::now();
            state.retain(|_, pending| now < pending.expires_at);
            state.insert(
                request_id,
                Pending {
                    task: SourceRelayTask {
                        request_id,
                        worker_id,
                        root,
                        source_name,
                    },
                    dispatched: false,
                    expires_at: now + timeout,
                    started: Some(started_tx),
                    sink,
                },
            );
        }
        self.inner.notify.notify_waiters();
        match tokio::time::timeout(timeout, started_rx).await {
            Ok(Ok(())) => Ok(stream),
            _ => {
                self.inner.state.lock().await.remove(&request_id);
                Err(AppError::CentralUnavailable)
            }
        }
    }

    async fn drain_pending(&self, worker_id: Uuid) -> Vec<SourceRelayTask> {
        let mut state = self.inner.state.lock().await;
        let now = tokio::time::Instant::now();
        state.retain(|_, pending| now < pending.expires_at);
        state
            .values_mut()
            .filter(|pending| pending.task.worker_id == worker_id && !pending.dispatched)
            .map(|pending| {
                pending.dispatched = true;
                pending.task.clone()
            })
            .collect()
    }

    /// Demandes en attente pour un worker source, avec attente longue.
    pub async fn take_for_worker(&self, worker_id: Uuid, wait: Duration) -> Vec<SourceRelayTask> {
        let tasks = self.drain_pending(worker_id).await;
        if !tasks.is_empty() || wait.is_zero() {
            return tasks;
        }
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let notified = self.inner.notify.notified();
            let tasks = self.drain_pending(worker_id).await;
            if !tasks.is_empty() {
                return tasks;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.drain_pending(worker_id).await;
            }
        }
    }

    /// Le worker source commence l'envoi : rend l'écrivain du flux et réveille le worker traitant.
    pub async fn begin(&self, request_id: Uuid, worker_id: Uuid) -> Option<RelaySink> {
        let mut state = self.inner.state.lock().await;
        let pending = state.get_mut(&request_id)?;
        if pending.task.worker_id != worker_id || !pending.dispatched {
            return None;
        }
        let started = pending.started.take()?;
        let sink = RelaySink {
            sink: pending.sink.clone(),
        };
        let _ = started.send(());
        state.remove(&request_id);
        Some(sink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: Duration = Duration::from_millis(200);

    #[tokio::test]
    async fn streams_chunks_from_the_source_worker_to_the_requester() -> anyhow::Result<()> {
        let broker = SourceRelayBroker::new();
        let source_worker = Uuid::new_v4();
        let requester = broker.clone();
        let waiting = tokio::spawn(async move {
            requester
                .request(
                    source_worker,
                    "/mnt/films".into(),
                    "a.mkv".into(),
                    SHORT * 10,
                )
                .await
        });
        let tasks = broker.take_for_worker(source_worker, SHORT * 10).await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].source_name, "a.mkv");
        let sink = broker
            .begin(tasks[0].request_id, source_worker)
            .await
            .ok_or_else(|| anyhow::anyhow!("l'envoi doit être accepté"))?;
        let mut stream = waiting.await??;
        assert!(sink.send(Ok(Bytes::from_static(b"abc"))).await);
        drop(sink);
        assert_eq!(
            stream.recv().await.transpose()?,
            Some(Bytes::from_static(b"abc"))
        );
        assert!(stream.recv().await.is_none(), "le flux se ferme à la fin");
        Ok(())
    }

    #[tokio::test]
    async fn times_out_when_no_source_worker_answers() {
        let broker = SourceRelayBroker::new();
        let result = broker
            .request(Uuid::new_v4(), "/r".into(), "a.mkv".into(), SHORT)
            .await;
        assert!(matches!(result, Err(AppError::CentralUnavailable)));
    }

    #[tokio::test]
    async fn only_the_addressed_worker_can_begin_a_transfer() -> anyhow::Result<()> {
        let broker = SourceRelayBroker::new();
        let source_worker = Uuid::new_v4();
        let requester = broker.clone();
        let _waiting = tokio::spawn(async move {
            requester
                .request(source_worker, "/r".into(), "a.mkv".into(), SHORT * 10)
                .await
        });
        let tasks = broker.take_for_worker(source_worker, SHORT * 10).await;
        let request_id = tasks[0].request_id;
        assert!(broker.begin(request_id, Uuid::new_v4()).await.is_none());
        assert!(broker.begin(request_id, source_worker).await.is_some());
        assert!(
            broker.begin(request_id, source_worker).await.is_none(),
            "une demande ne démarre qu'une fois"
        );
        Ok(())
    }
}
