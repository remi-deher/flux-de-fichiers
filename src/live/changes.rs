//! Signalement regroupé des changements : au plus un événement par sujet et par fenêtre.

use super::{EventHistory, LiveEvent};
use forgearr_shared::live::ChangeTopic;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

/// Durée de regroupement : l'événement part à la fin de la fenêtre ouverte par le premier changement.
pub const CHANGE_WINDOW: Duration = Duration::from_millis(300);

/// Signale qu'un sujet a changé, une fois son écriture validée.
#[derive(Clone, Debug, Default)]
pub struct ChangeNotifier {
    history: EventHistory,
    pending: Arc<Mutex<HashSet<ChangeTopic>>>,
}

impl ChangeNotifier {
    pub fn new(history: EventHistory) -> Self {
        Self {
            history,
            pending: Arc::default(),
        }
    }

    fn pending(&self) -> MutexGuard<'_, HashSet<ChangeTopic>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// À appeler après l'écriture. Le dernier changement n'est jamais perdu : la fenêtre se ferme
    /// avant la publication, donc un changement plus tardif en ouvre une autre.
    pub fn notify(&self, topic: ChangeTopic) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(?topic, "changement non signalé : hors du moteur asynchrone");
            return;
        };
        if !self.pending().insert(topic) {
            return;
        }
        let notifier = self.clone();
        runtime.spawn(async move {
            tokio::time::sleep(CHANGE_WINDOW).await;
            notifier.pending().remove(&topic);
            notifier.history.publish(LiveEvent::Changed(topic)).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: Duration = Duration::from_secs(5);

    async fn next_topic(
        rx: &mut tokio::sync::broadcast::Receiver<(u64, LiveEvent)>,
    ) -> anyhow::Result<ChangeTopic> {
        match tokio::time::timeout(LIMIT, rx.recv()).await?? {
            (_, LiveEvent::Changed(topic)) => Ok(topic),
            (_, LiveEvent::Job(_)) => anyhow::bail!("événement de tâche inattendu"),
        }
    }

    #[tokio::test]
    async fn changes_of_a_window_are_grouped_by_topic() -> anyhow::Result<()> {
        let history = EventHistory::new();
        let changes = ChangeNotifier::new(history.clone());
        let mut rx = history.subscribe_from(None).await.rx;
        for _ in 0..10 {
            changes.notify(ChangeTopic::Catalog);
        }
        changes.notify(ChangeTopic::Tools);
        let mut topics = [next_topic(&mut rx).await?, next_topic(&mut rx).await?];
        topics.sort_by_key(|topic| *topic as u8);
        assert_eq!(topics, [ChangeTopic::Catalog, ChangeTopic::Tools]);
        tokio::time::sleep(CHANGE_WINDOW * 2).await;
        assert!(rx.try_recv().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn a_change_after_the_window_is_not_lost() -> anyhow::Result<()> {
        let history = EventHistory::new();
        let changes = ChangeNotifier::new(history.clone());
        let mut rx = history.subscribe_from(None).await.rx;
        for _ in 0..2 {
            changes.notify(ChangeTopic::Catalog);
            assert_eq!(next_topic(&mut rx).await?, ChangeTopic::Catalog);
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_blocking_task_can_signal_a_change() -> anyhow::Result<()> {
        let history = EventHistory::new();
        let changes = ChangeNotifier::new(history.clone());
        let mut rx = history.subscribe_from(None).await.rx;
        tokio::task::spawn_blocking(move || changes.notify(ChangeTopic::Tools)).await?;
        assert_eq!(next_topic(&mut rx).await?, ChangeTopic::Tools);
        Ok(())
    }
}
