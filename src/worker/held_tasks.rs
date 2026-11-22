//! Tâches que ce processus tient, jusqu'à l'accusé de fin ; annoncées à chaque réservation (ADR 0027).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};
use uuid::Uuid;

/// Tâches par jeton de bail : une même tâche peut être reprise avant la fin de son ancien bail.
#[derive(Clone, Default)]
pub(super) struct HeldTasks(Arc<Mutex<HashMap<Uuid, Uuid>>>);

/// Tâche tenue ; retirée à l'abandon de cette valeur.
pub(super) struct HeldTask {
    tasks: HeldTasks,
    token: Uuid,
}

impl HeldTasks {
    fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, Uuid>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn hold(&self, job: Uuid, token: Uuid) -> HeldTask {
        self.lock().insert(token, job);
        HeldTask {
            tasks: self.clone(),
            token,
        }
    }

    pub(super) fn jobs(&self) -> Vec<Uuid> {
        self.lock().values().copied().collect()
    }
}

impl Drop for HeldTask {
    fn drop(&mut self) {
        self.tasks.lock().remove(&self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_is_held_until_its_own_lease_ends() {
        let tasks = HeldTasks::default();
        let job = Uuid::new_v4();
        let old = tasks.hold(job, Uuid::new_v4());
        let new = tasks.hold(job, Uuid::new_v4());
        drop(old);
        assert_eq!(tasks.jobs(), [job]);
        drop(new);
        assert!(tasks.jobs().is_empty());
    }
}
