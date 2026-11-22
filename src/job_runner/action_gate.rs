//! Accord du central avant une action irréversible (ADR 0027), demandé de la même façon par le worker
//! distant et le worker intégré.

use crate::domain::{IrreversibleAction, JobErrorCode};
use std::{future::Future, pin::Pin, sync::OnceLock};

/// Arrêt d'une tâche sans échec à déclarer : le central décide, ou a déjà décidé, de la suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TaskStop {
    #[error("bail perdu")]
    LeaseLost,
    #[error("central injoignable au-delà du délai")]
    Unreachable,
    #[error("arrêt du worker")]
    Shutdown,
    /// L'issue est déjà inscrite au central.
    #[error("issue déjà inscrite au central")]
    Concluded,
}

/// Refus d'une action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionRefusal {
    /// La tâche s'arrête.
    Stop(TaskStop),
    /// Le bail vaut toujours, mais cette action est exclue.
    Excluded(String),
}

impl ActionRefusal {
    pub fn message(&self) -> String {
        match self {
            Self::Stop(stop) => stop.to_string(),
            Self::Excluded(reason) => reason.clone(),
        }
    }
}

/// Action dont l'accord est demandé.
#[derive(Clone, Copy, Debug)]
pub struct ActionRequest<'a> {
    pub action: IrreversibleAction,
    pub node_id: &'a str,
    /// Code de l'échec, pour le flux d'échec.
    pub error_code: Option<JobErrorCode>,
}

impl<'a> ActionRequest<'a> {
    pub fn block(action: IrreversibleAction, node_id: &'a str) -> Self {
        Self {
            action,
            node_id,
            error_code: None,
        }
    }
}

pub type GateFuture<'a> = Pin<Box<dyn Future<Output = Result<(), ActionRefusal>> + Send + 'a>>;

pub trait ActionGate: Send + Sync {
    /// Demande l'accord ; patiente tant que le central est injoignable, dans la limite du délai.
    fn authorize<'a>(&'a self, request: ActionRequest<'a>) -> GateFuture<'a>;

    /// Arrêt constaté par une demande précédente.
    fn halt(&self) -> Option<TaskStop>;
}

/// Premier arrêt constaté par une tâche.
#[derive(Debug, Default)]
pub struct StopMemory(OnceLock<TaskStop>);

impl StopMemory {
    pub fn record(&self, stop: TaskStop) {
        // Seul le premier arrêt compte.
        let _ = self.0.set(stop);
    }

    pub fn get(&self) -> Option<TaskStop> {
        self.0.get().copied()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::{Mutex, PoisonError};

    /// Accord de test : réponse fixe et demandes reçues.
    pub struct FakeGate {
        pub answer: Result<(), ActionRefusal>,
        pub asked: Mutex<Vec<(IrreversibleAction, String)>>,
        stop: StopMemory,
    }

    impl FakeGate {
        pub fn new(answer: Result<(), ActionRefusal>) -> Self {
            Self {
                answer,
                asked: Mutex::default(),
                stop: StopMemory::default(),
            }
        }

        pub fn asked(&self) -> Vec<(IrreversibleAction, String)> {
            self.asked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl ActionGate for FakeGate {
        fn authorize<'a>(&'a self, request: ActionRequest<'a>) -> GateFuture<'a> {
            self.asked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((request.action, request.node_id.to_owned()));
            if let Err(ActionRefusal::Stop(stop)) = self.answer {
                self.stop.record(stop);
            }
            Box::pin(std::future::ready(self.answer.clone()))
        }

        fn halt(&self) -> Option<TaskStop> {
            self.stop.get()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_stop_is_remembered() {
        let memory = StopMemory::default();
        assert_eq!(memory.get(), None);
        memory.record(TaskStop::LeaseLost);
        memory.record(TaskStop::Unreachable);
        assert_eq!(memory.get(), Some(TaskStop::LeaseLost));
    }
}
