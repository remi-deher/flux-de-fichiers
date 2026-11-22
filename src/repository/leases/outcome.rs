//! Motifs, issues et changements à notifier d'une fin de bail.

use crate::domain::{JobErrorCode, JobEventType, JobStatus};
use uuid::Uuid;

/// Pourquoi un bail se termine sans que son titulaire l'ait conclu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseEnd {
    /// Échéance passée.
    Expired,
    /// Le processus titulaire n'exécute plus la tâche (redémarrage, tâche désavouée).
    HolderStopped,
    /// Le titulaire rend la tâche (arrêt propre).
    Released,
    /// Le bail n'a jamais été remis au worker (erreur passagère du central).
    NotDelivered,
}

impl LeaseEnd {
    /// La tentative n'est pas imputable à la tâche.
    pub(in crate::repository) const fn refunds_attempt(self) -> bool {
        matches!(self, Self::Released | Self::NotDelivered)
    }
}

/// Issue d'une fin de bail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseOutcome {
    Requeued,
    Failed(JobErrorCode),
    /// Publication en cours : le bail subsiste jusqu'à sa conclusion ou l'échéance.
    Deferred,
}

/// Changement d'une tâche à notifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaseChange {
    pub job_id: Uuid,
    pub event: JobEventType,
    pub status: JobStatus,
    pub error_code: Option<JobErrorCode>,
    /// Une publication reste à conclure sans son titulaire.
    pub deferred: bool,
}

impl LeaseChange {
    pub(in crate::repository) fn unreachable(job_id: Uuid) -> Self {
        Self {
            job_id,
            event: JobEventType::JobUnreachable,
            status: JobStatus::Running,
            error_code: None,
            deferred: false,
        }
    }

    pub(in crate::repository) fn from_outcome(job_id: Uuid, outcome: LeaseOutcome) -> Self {
        let base = Self::unreachable(job_id);
        match outcome {
            LeaseOutcome::Requeued => Self {
                event: JobEventType::JobRequeued,
                status: JobStatus::Queued,
                ..base
            },
            LeaseOutcome::Failed(code) => Self {
                event: JobEventType::JobFailed,
                status: JobStatus::Failed,
                error_code: Some(code),
                ..base
            },
            LeaseOutcome::Deferred => Self {
                deferred: true,
                ..base
            },
        }
    }
}
