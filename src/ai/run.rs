//! Conversations de l'assistant lancées en tâche de fond, dont les évènements sont publiés en SSE.
//!
//! Un « run » garde en mémoire (jamais en base) l'historique de ses évènements : un client qui se
//! reconnecte reprend là où il s'était arrêté (`Last-Event-ID`). Chaque run appartient à un compte.

use flux_shared::ai::AiRunEvent;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};
use tokio::{
    sync::{Notify, oneshot},
    task::AbortHandle,
};

/// Durée de conservation d'un run terminé : le temps de relire sa fin après une coupure réseau.
const FINISHED_RUN_TTL: Duration = Duration::from_secs(600);
/// Durée maximale d'un run, terminé ou non.
const MAX_RUN_AGE: Duration = Duration::from_secs(1800);
/// Runs simultanés autorisés par compte.
pub const MAX_ACTIVE_RUNS_PER_OWNER: usize = 3;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Un run : son propriétaire, ses évènements et de quoi l'interrompre.
pub struct Run {
    owner: String,
    created: Instant,
    events: Mutex<Vec<AiRunEvent>>,
    finished_at: Mutex<Option<Instant>>,
    changed: Notify,
    abort: Mutex<Option<AbortHandle>>,
    /// Actions d'écriture en attente de la réponse de l'utilisateur.
    decisions: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl Run {
    fn new(owner: String) -> Self {
        Self {
            owner,
            created: Instant::now(),
            events: Mutex::new(Vec::new()),
            finished_at: Mutex::new(None),
            changed: Notify::new(),
            abort: Mutex::new(None),
            decisions: Mutex::new(HashMap::new()),
        }
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn is_finished(&self) -> bool {
        lock(&self.finished_at).is_some()
    }

    /// Ajoute un évènement ; un évènement final clôt le run. Sans effet une fois clos.
    pub fn publish(&self, event: AiRunEvent) {
        if self.is_finished() {
            return;
        }
        let is_final = event.is_final();
        lock(&self.events).push(event);
        if is_final {
            *lock(&self.finished_at) = Some(Instant::now());
        }
        self.changed.notify_waiters();
    }

    /// Évènement d'indice `index`, ou `None` s'il n'existe pas encore.
    fn event_at(&self, index: usize) -> Option<AiRunEvent> {
        lock(&self.events).get(index).cloned()
    }

    /// Prochain évènement à partir de `index` ; `None` quand le run est clos et tout a été lu.
    pub async fn next_event(&self, index: usize) -> Option<AiRunEvent> {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(event) = self.event_at(index) {
                return Some(event);
            }
            if self.is_finished() {
                return None;
            }
            notified.await;
        }
    }

    /// Suspend le run jusqu'à la réponse de l'utilisateur à `action_id` : `true` pour autoriser.
    /// Sans réponse dans `timeout`, ou si le run est interrompu, l'action est refusée.
    pub async fn await_decision(&self, action_id: &str, timeout: Duration) -> bool {
        let (sender, receiver) = oneshot::channel();
        lock(&self.decisions).insert(action_id.to_owned(), sender);
        let approved = matches!(tokio::time::timeout(timeout, receiver).await, Ok(Ok(true)));
        lock(&self.decisions).remove(action_id);
        approved
    }

    /// Transmet la réponse de l'utilisateur ; `false` si l'action n'attend (plus) de réponse.
    pub fn resolve(&self, action_id: &str, approve: bool) -> bool {
        lock(&self.decisions)
            .remove(action_id)
            .is_some_and(|sender| sender.send(approve).is_ok())
    }

    pub fn attach_abort(&self, handle: AbortHandle) {
        *lock(&self.abort) = Some(handle);
    }

    /// Interrompt la tâche et clôt le run avec `event`.
    pub fn cancel(&self, event: AiRunEvent) {
        if let Some(handle) = lock(&self.abort).take() {
            handle.abort();
        }
        self.publish(event);
    }

    fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.created) > MAX_RUN_AGE
            || lock(&self.finished_at)
                .is_some_and(|finished| now.duration_since(finished) > FINISHED_RUN_TTL)
    }
}

/// Registre des runs en cours ou récemment terminés.
#[derive(Clone, Default)]
pub struct AiRuns {
    inner: Arc<Mutex<HashMap<String, Arc<Run>>>>,
}

impl AiRuns {
    pub fn new() -> Self {
        Self::default()
    }

    /// Crée un run pour `owner`, ou `None` si ce compte a déjà trop de runs actifs.
    pub fn create(&self, owner: &str) -> Option<(String, Arc<Run>)> {
        let mut runs = lock(&self.inner);
        let now = Instant::now();
        runs.retain(|_, run| !run.is_expired(now));
        let active = runs
            .values()
            .filter(|run| run.owner() == owner && !run.is_finished())
            .count();
        if active >= MAX_ACTIVE_RUNS_PER_OWNER {
            return None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let run = Arc::new(Run::new(owner.to_owned()));
        runs.insert(id.clone(), Arc::clone(&run));
        Some((id, run))
    }

    /// Run `id`, seulement s'il appartient à `owner`.
    pub fn get(&self, id: &str, owner: &str) -> Option<Arc<Run>> {
        lock(&self.inner)
            .get(id)
            .filter(|run| run.owner() == owner)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::ai::AiRunEvent;

    fn started(name: &str) -> AiRunEvent {
        AiRunEvent::ToolStarted {
            tool_name: name.into(),
        }
    }

    fn open(runs: &AiRuns, owner: &str) -> anyhow::Result<(String, Arc<Run>)> {
        runs.create(owner)
            .ok_or_else(|| anyhow::anyhow!("run refusé"))
    }

    fn failed() -> AiRunEvent {
        AiRunEvent::Failed {
            message: "x".into(),
        }
    }

    #[tokio::test]
    async fn events_are_replayed_from_any_index_and_the_final_one_closes_the_run()
    -> anyhow::Result<()> {
        let runs = AiRuns::new();
        let (_, run) = open(&runs, "alice")?;
        run.publish(started("a"));
        run.publish(failed());
        run.publish(started("ignoré"));
        assert_eq!(run.next_event(0).await, Some(started("a")));
        assert_eq!(run.next_event(1).await, Some(failed()));
        assert_eq!(run.next_event(2).await, None);
        assert!(run.is_finished());
        Ok(())
    }

    #[tokio::test]
    async fn a_waiting_reader_is_woken_by_a_new_event() -> anyhow::Result<()> {
        let runs = AiRuns::new();
        let (_, run) = open(&runs, "alice")?;
        let reader = {
            let run = Arc::clone(&run);
            tokio::spawn(async move { run.next_event(0).await })
        };
        tokio::task::yield_now().await;
        run.publish(started("b"));
        assert_eq!(reader.await?, Some(started("b")));
        Ok(())
    }

    #[tokio::test]
    async fn runs_are_private_to_their_owner_and_capped_per_owner() -> anyhow::Result<()> {
        let runs = AiRuns::new();
        let (id, _) = open(&runs, "alice")?;
        assert!(runs.get(&id, "alice").is_some());
        assert!(runs.get(&id, "bob").is_none());
        for _ in 1..MAX_ACTIVE_RUNS_PER_OWNER {
            assert!(runs.create("alice").is_some());
        }
        assert!(runs.create("alice").is_none());
        assert!(runs.create("bob").is_some());
        Ok(())
    }

    #[tokio::test]
    async fn a_decision_resumes_the_waiting_run_and_silence_means_refusal() -> anyhow::Result<()> {
        let runs = AiRuns::new();
        let (_, run) = open(&runs, "alice")?;
        let waiting = {
            let run = Arc::clone(&run);
            tokio::spawn(async move { run.await_decision("a1", Duration::from_secs(5)).await })
        };
        while !run.resolve("a1", true) {
            tokio::task::yield_now().await;
        }
        assert!(waiting.await?);
        // Une action déjà résolue, ou inconnue, n'attend plus rien.
        assert!(!run.resolve("a1", true));
        assert!(!run.await_decision("a2", Duration::from_millis(20)).await);
        let refused = {
            let run = Arc::clone(&run);
            tokio::spawn(async move { run.await_decision("a3", Duration::from_secs(5)).await })
        };
        while !run.resolve("a3", false) {
            tokio::task::yield_now().await;
        }
        assert!(!refused.await?);
        Ok(())
    }

    #[tokio::test]
    async fn cancel_closes_the_run_with_the_given_event() -> anyhow::Result<()> {
        let runs = AiRuns::new();
        let (_, run) = open(&runs, "alice")?;
        let task = tokio::spawn(std::future::pending::<()>());
        run.attach_abort(task.abort_handle());
        run.cancel(failed());
        assert_eq!(run.next_event(0).await, Some(failed()));
        assert!(task.await.is_err());
        Ok(())
    }
}
