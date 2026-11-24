//! Courtier en mémoire des demandes que le central confie à un worker distant connecté : exploration de
//! dossiers (ADR 0014), essai d'un script (ADR 0031). Une demande attend en file ; le worker la prend par
//! attente longue, puis renvoie sa réponse, qui réveille l'appel HTTP en attente.

use crate::error::{AppError, AppResult};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Notify, oneshot};
use uuid::Uuid;

/// Durée de vie minimale d'une demande en file, même si l'appelant n'attend pas aussi longtemps.
pub const MIN_REQUEST_TTL: Duration = Duration::from_secs(30);

/// Une demande ou une réponse relayée : elle porte son identifiant et le worker concerné.
pub trait Relayed {
    fn request_id(&self) -> Uuid;
    fn worker_id(&self) -> Uuid;
}

struct Pending<T, R> {
    task: T,
    dispatched: bool,
    expires_at: tokio::time::Instant,
    reply: oneshot::Sender<R>,
}

struct Inner<T, R> {
    state: Mutex<HashMap<Uuid, Pending<T, R>>>,
    notify: Notify,
}

/// Courtier de demandes `T` dont les réponses sont des `R`.
pub struct RequestBroker<T, R> {
    inner: Arc<Inner<T, R>>,
}

impl<T, R> Clone for RequestBroker<T, R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T, R> Default for RequestBroker<T, R> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(HashMap::new()),
                notify: Notify::new(),
            }),
        }
    }
}

impl<T: Relayed + Clone, R: Relayed> RequestBroker<T, R> {
    pub fn new() -> Self {
        Self::default()
    }

    fn purge_expired(state: &mut HashMap<Uuid, Pending<T, R>>) {
        let now = tokio::time::Instant::now();
        state.retain(|_, pending| now < pending.expires_at);
    }

    /// Met `task` en file pour son worker et attend la réponse jusqu'à `timeout`.
    pub async fn call(&self, task: T, timeout: Duration) -> AppResult<R> {
        let (tx, rx) = oneshot::channel();
        let request_id = task.request_id();
        {
            let mut state = self.inner.state.lock().await;
            Self::purge_expired(&mut state);
            state.insert(
                request_id,
                Pending {
                    task,
                    dispatched: false,
                    expires_at: tokio::time::Instant::now() + timeout.max(MIN_REQUEST_TTL),
                    reply: tx,
                },
            );
        }
        self.inner.notify.notify_waiters();

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(report)) => Ok(report),
            Ok(Err(_)) => Err(AppError::CentralUnavailable),
            Err(_) => {
                self.inner.state.lock().await.remove(&request_id);
                Err(AppError::CentralUnavailable)
            }
        }
    }

    async fn drain_pending(&self, worker_id: Uuid) -> Vec<T> {
        let mut state = self.inner.state.lock().await;
        Self::purge_expired(&mut state);
        let mut tasks = Vec::new();
        for pending in state.values_mut() {
            if pending.task.worker_id() == worker_id && !pending.dispatched {
                pending.dispatched = true;
                tasks.push(pending.task.clone());
            }
        }
        tasks
    }

    /// Récupère les demandes en attente pour un worker, avec attente longue optionnelle.
    pub async fn take_for_worker(&self, worker_id: Uuid, wait: Duration) -> Vec<T> {
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
            if tokio::time::Instant::now() >= deadline {
                return Vec::new();
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.drain_pending(worker_id).await;
            }
        }
    }

    /// Délivre la réponse du worker à l'appel en attente ; `false` si la demande est inconnue, expirée ou
    /// confiée à un autre worker.
    pub async fn complete(&self, report: R) -> bool {
        let entry = {
            let mut state = self.inner.state.lock().await;
            let matches_worker = state
                .get(&report.request_id())
                .is_some_and(|pending| pending.task.worker_id() == report.worker_id());
            if !matches_worker {
                return false;
            }
            state.remove(&report.request_id())
        };
        entry.is_some_and(|pending| pending.reply.send(report).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Ask {
        request: Uuid,
        worker: Uuid,
        text: &'static str,
    }

    struct Answer {
        request: Uuid,
        worker: Uuid,
        text: String,
    }

    impl Relayed for Ask {
        fn request_id(&self) -> Uuid {
            self.request
        }
        fn worker_id(&self) -> Uuid {
            self.worker
        }
    }

    impl Relayed for Answer {
        fn request_id(&self) -> Uuid {
            self.request
        }
        fn worker_id(&self) -> Uuid {
            self.worker
        }
    }

    type TestBroker = RequestBroker<Ask, Answer>;

    fn ask(worker: Uuid) -> Ask {
        Ask {
            request: Uuid::new_v4(),
            worker,
            text: "bonjour",
        }
    }

    fn answer(to: &Ask, worker: Uuid) -> Answer {
        Answer {
            request: to.request,
            worker,
            text: format!("re : {}", to.text),
        }
    }

    #[tokio::test]
    async fn a_request_reaches_its_worker_and_the_reply_wakes_the_caller() -> anyhow::Result<()> {
        let broker = TestBroker::new();
        let worker = Uuid::new_v4();
        let waiting = ask(worker);
        let caller = {
            let broker = broker.clone();
            let waiting = waiting.clone();
            tokio::spawn(async move {
                broker.call(waiting, Duration::from_secs(5)).await
            })
        };
        let taken = broker.take_for_worker(worker, Duration::from_secs(5)).await;
        assert_eq!(taken.len(), 1);
        assert!(broker.complete(answer(&waiting, worker)).await);
        let reply = caller.await??;
        assert_eq!(reply.text, "re : bonjour");
        Ok(())
    }

    #[tokio::test]
    async fn another_worker_sees_nothing_and_cannot_answer() -> anyhow::Result<()> {
        let broker = TestBroker::new();
        let (worker, stranger) = (Uuid::new_v4(), Uuid::new_v4());
        let waiting = ask(worker);
        let caller = {
            let broker = broker.clone();
            let waiting = waiting.clone();
            tokio::spawn(async move {
                broker.call(waiting, Duration::from_millis(300)).await
            })
        };
        let seen = broker.take_for_worker(stranger, Duration::ZERO).await;
        assert!(seen.is_empty());
        assert!(!broker.complete(answer(&waiting, stranger)).await);
        assert!(matches!(caller.await?, Err(AppError::CentralUnavailable)));
        Ok(())
    }

    #[tokio::test]
    async fn a_request_is_handed_out_once_and_dropped_after_its_timeout() -> anyhow::Result<()> {
        let broker = TestBroker::new();
        let worker = Uuid::new_v4();
        let waiting = ask(worker);
        let outcome = broker.call(waiting.clone(), Duration::from_millis(50)).await;
        assert!(matches!(outcome, Err(AppError::CentralUnavailable)));
        let seen = broker.take_for_worker(worker, Duration::ZERO).await;
        assert!(seen.is_empty());
        assert!(!broker.complete(answer(&waiting, worker)).await);
        Ok(())
    }
}
