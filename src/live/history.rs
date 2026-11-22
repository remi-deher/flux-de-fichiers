//! Historique borné des événements, pour la reprise SSE via `Last-Event-ID`.

use crate::domain::JobEvent;
use forgearr_shared::live::ChangeTopic;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{Mutex, broadcast};

const REPLAY_CAPACITY: usize = 256;
const BROADCAST_CAPACITY: usize = 256;

/// Événement numéroté du flux : une tâche, ou un sujet à relire.
#[derive(Clone, Debug)]
pub enum LiveEvent {
    Job(JobEvent),
    Changed(ChangeTopic),
}

impl From<JobEvent> for LiveEvent {
    fn from(event: JobEvent) -> Self {
        Self::Job(event)
    }
}

#[derive(Debug)]
struct Recent {
    next_seq: u64,
    buffer: VecDeque<(u64, LiveEvent)>,
}

/// Abonnement : ce qui a été manqué, puis la suite.
#[derive(Debug)]
pub struct Subscription {
    pub replay: VecDeque<(u64, LiveEvent)>,
    pub rx: broadcast::Receiver<(u64, LiveEvent)>,
    /// Numéro à partir duquel cet abonnement est complet.
    pub position: u64,
    /// Des événements manqués ne sont plus connus : le client doit tout relire.
    pub resync: bool,
}

/// Tampon d'historique borné, partagé par les flux d'événements.
#[derive(Clone, Debug)]
pub struct EventHistory {
    recent: Arc<Mutex<Recent>>,
    tx: broadcast::Sender<(u64, LiveEvent)>,
}

impl Default for EventHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHistory {
    /// Les numéros partent de l'horloge : ceux d'un central redémarré dépassent toujours ceux du
    /// précédent, dont le `Last-Event-ID` est ainsi reconnu comme étranger.
    pub fn new() -> Self {
        Self::starting_at(u64::try_from(chrono::Utc::now().timestamp_micros()).unwrap_or(1))
    }

    fn starting_at(first_seq: u64) -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            recent: Arc::new(Mutex::new(Recent {
                next_seq: first_seq,
                buffer: VecDeque::with_capacity(REPLAY_CAPACITY),
            })),
            tx,
        }
    }

    pub async fn publish(&self, event: impl Into<LiveEvent>) -> u64 {
        let event = event.into();
        let mut recent = self.recent.lock().await;
        let seq = recent.next_seq;
        recent.next_seq += 1;
        recent.buffer.push_back((seq, event.clone()));
        while recent.buffer.len() > REPLAY_CAPACITY {
            recent.buffer.pop_front();
        }
        let _ = self.tx.send((seq, event));
        seq
    }

    /// S'abonne après `last_event_id`. Un numéro trop ancien, ou d'une autre instance, impose une
    /// relecture complète ; les événements de tâches encore connus sont alors rejoués.
    pub async fn subscribe_from(&self, last_event_id: Option<u64>) -> Subscription {
        let recent = self.recent.lock().await;
        let rx = self.tx.subscribe();
        let last = recent.next_seq.saturating_sub(1);
        let oldest = recent
            .buffer
            .front()
            .map_or(recent.next_seq, |(seq, _)| *seq);
        let before_oldest = oldest.saturating_sub(1);
        match last_event_id {
            None => Subscription {
                replay: VecDeque::new(),
                rx,
                position: last,
                resync: false,
            },
            Some(since) if (before_oldest..=last).contains(&since) => Subscription {
                replay: recent
                    .buffer
                    .iter()
                    .filter(|(seq, _)| *seq > since)
                    .cloned()
                    .collect(),
                rx,
                position: since,
                resync: false,
            },
            Some(_) => Subscription {
                replay: recent
                    .buffer
                    .iter()
                    .filter(|(_, event)| matches!(event, LiveEvent::Job(_)))
                    .cloned()
                    .collect(),
                rx,
                position: before_oldest,
                resync: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_SEQ: u64 = 1_000;
    const FOREIGN_SEQ: u64 = 1_050;
    const CHANGED: LiveEvent = LiveEvent::Changed(ChangeTopic::Catalog);

    #[tokio::test]
    async fn a_known_position_is_replayed_without_resync() {
        let history = EventHistory::starting_at(FIRST_SEQ);
        let first = history.publish(CHANGED).await;
        let second = history.publish(CHANGED).await;
        let resumed = history.subscribe_from(Some(first)).await;
        assert!(!resumed.resync);
        assert_eq!(resumed.position, first);
        let replayed: Vec<u64> = resumed.replay.iter().map(|(seq, _)| *seq).collect();
        assert_eq!(replayed, [second]);
        let up_to_date = history.subscribe_from(Some(second)).await;
        assert!(!up_to_date.resync && up_to_date.replay.is_empty());
    }

    #[tokio::test]
    async fn a_fresh_subscription_gets_a_position_it_can_resume_from() {
        let history = EventHistory::starting_at(FIRST_SEQ);
        let fresh = history.subscribe_from(None).await;
        assert!(!fresh.resync && fresh.replay.is_empty());
        history.publish(CHANGED).await;
        let resumed = history.subscribe_from(Some(fresh.position)).await;
        assert!(!resumed.resync);
        assert_eq!(resumed.replay.len(), 1);
    }

    #[tokio::test]
    async fn a_position_of_another_instance_asks_for_a_resync() {
        let history = EventHistory::starting_at(FIRST_SEQ);
        for foreign in [1, FOREIGN_SEQ, u64::MAX] {
            assert!(history.subscribe_from(Some(foreign)).await.resync);
        }
        let mut subscription = history.subscribe_from(Some(FOREIGN_SEQ)).await;
        assert_eq!(subscription.position, FIRST_SEQ - 1);
        let seq = history.publish(CHANGED).await;
        // Le numéro présenté ne masque pas les nouveaux événements.
        assert!(
            subscription
                .rx
                .try_recv()
                .is_ok_and(|(received, _)| received == seq)
        );
    }

    #[tokio::test]
    async fn a_restarted_central_numbers_after_the_previous_one() {
        let before = EventHistory::new().publish(CHANGED).await;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let restarted = EventHistory::new();
        assert!(restarted.subscribe_from(Some(before)).await.resync);
        assert!(restarted.publish(CHANGED).await > before);
    }

    #[tokio::test]
    async fn events_out_of_the_buffer_ask_for_a_resync() {
        let history = EventHistory::starting_at(FIRST_SEQ);
        let first = history.publish(CHANGED).await;
        for _ in 0..=REPLAY_CAPACITY {
            history.publish(CHANGED).await;
        }
        let late = history.subscribe_from(Some(first)).await;
        assert!(late.resync);
        // La relecture complète remplace les changements rejoués.
        assert!(late.replay.is_empty());
    }
}
