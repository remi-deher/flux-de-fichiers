//! Places d'exécution d'un worker par classe de ressource (CPU, GPU).
//!
//! Sans place par classe déclarée, le worker garde le mode historique : `max_concurrency`
//! places communes à toutes les tâches. Dès qu'une limite par classe est renseignée, chaque
//! classe a ses propres places (`max_cpu_jobs` vaut alors `max_concurrency` par défaut, et
//! `max_gpu_jobs` vaut 0, ce qui écarte les tâches GPU) et le total est leur somme.

use super::{WORKER_MAX_CONCURRENCY, WORKER_MIN_CONCURRENCY};
use crate::jobs::JobResourceClass;

/// Une classe à 0 place n'est jamais réclamée par ce worker.
pub const WORKER_MIN_CLASS_SLOTS: i32 = 0;
pub const WORKER_MAX_CLASS_SLOTS: i32 = WORKER_MAX_CONCURRENCY;

/// Places GPU d'un worker qui ne déclare que ses places CPU.
const UNDECLARED_GPU_SLOTS: i32 = 0;

/// Tâches en cours d'un worker, comptées par classe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunningJobs {
    pub cpu: i64,
    pub gpu: i64,
}

impl RunningJobs {
    pub fn total(self) -> i64 {
        self.cpu + self.gpu
    }

    pub fn of(self, class: JobResourceClass) -> i64 {
        match class {
            JobResourceClass::Cpu => self.cpu,
            JobResourceClass::Gpu => self.gpu,
        }
    }

    pub fn add(&mut self, class: JobResourceClass, count: i64) {
        match class {
            JobResourceClass::Cpu => self.cpu += count,
            JobResourceClass::Gpu => self.gpu += count,
        }
    }
}

/// Places effectives d'un worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerSlots {
    pub cpu: i32,
    pub gpu: i32,
    /// Tâches simultanées au plus, toutes classes confondues.
    pub total: i32,
}

impl WorkerSlots {
    pub fn of(self, class: JobResourceClass) -> i32 {
        match class {
            JobResourceClass::Cpu => self.cpu,
            JobResourceClass::Gpu => self.gpu,
        }
    }

    /// Une tâche de `class` peut démarrer : place libre dans sa classe et dans le total.
    pub fn admits(self, class: JobResourceClass, running: RunningJobs) -> bool {
        running.total() < i64::from(self.total) && running.of(class) < i64::from(self.of(class))
    }

    /// Classes encore réclamables, dans l'ordre CPU puis GPU.
    pub fn admitted_classes(self, running: RunningJobs) -> Vec<JobResourceClass> {
        [JobResourceClass::Cpu, JobResourceClass::Gpu]
            .into_iter()
            .filter(|class| self.admits(*class, running))
            .collect()
    }
}

/// Places effectives à partir des colonnes brutes du worker (règle unique, voir l'en-tête).
pub fn worker_slots(
    max_concurrency: i32,
    max_cpu_jobs: Option<i32>,
    max_gpu_jobs: Option<i32>,
) -> WorkerSlots {
    let common = max_concurrency.clamp(WORKER_MIN_CONCURRENCY, WORKER_MAX_CONCURRENCY);
    let bound = |slots: i32| slots.clamp(WORKER_MIN_CLASS_SLOTS, WORKER_MAX_CLASS_SLOTS);
    if max_cpu_jobs.is_none() && max_gpu_jobs.is_none() {
        return WorkerSlots {
            cpu: common,
            gpu: common,
            total: common,
        };
    }
    let cpu = bound(max_cpu_jobs.unwrap_or(common));
    let gpu = bound(max_gpu_jobs.unwrap_or(UNDECLARED_GPU_SLOTS));
    WorkerSlots {
        cpu,
        gpu,
        total: cpu + gpu,
    }
}

/// Limites par classe acceptables : chacune dans ses bornes, et au moins une place au total.
pub fn class_slots_valid(max_cpu_jobs: Option<i32>, max_gpu_jobs: Option<i32>) -> bool {
    let in_bounds = |slots: Option<i32>| {
        slots.is_none_or(|value| (WORKER_MIN_CLASS_SLOTS..=WORKER_MAX_CLASS_SLOTS).contains(&value))
    };
    in_bounds(max_cpu_jobs)
        && in_bounds(max_gpu_jobs)
        && worker_slots(WORKER_MIN_CONCURRENCY, max_cpu_jobs, max_gpu_jobs).total > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPU: JobResourceClass = JobResourceClass::Cpu;
    const GPU: JobResourceClass = JobResourceClass::Gpu;

    fn running(cpu: i64, gpu: i64) -> RunningJobs {
        RunningJobs { cpu, gpu }
    }

    #[test]
    fn legacy_mode_shares_max_concurrency_between_classes() {
        let slots = worker_slots(1, None, None);
        assert_eq!(slots, WorkerSlots { cpu: 1, gpu: 1, total: 1 });
        assert!(slots.admits(CPU, running(0, 0)));
        assert!(!slots.admits(CPU, running(0, 1)));
        assert!(slots.admitted_classes(running(0, 1)).is_empty());
        assert_eq!(slots.admitted_classes(running(0, 0)), vec![CPU, GPU]);
    }

    #[test]
    fn cpu_only_declaration_disables_gpu() {
        let slots = worker_slots(3, Some(2), None);
        assert_eq!(slots, WorkerSlots { cpu: 2, gpu: 0, total: 2 });
        assert!(!slots.admits(GPU, running(0, 0)));
        assert_eq!(slots.admitted_classes(running(1, 0)), vec![CPU]);
    }

    #[test]
    fn gpu_only_declaration_keeps_max_concurrency_for_cpu() {
        let slots = worker_slots(2, None, Some(1));
        assert_eq!(slots, WorkerSlots { cpu: 2, gpu: 1, total: 3 });
    }

    #[test]
    fn separate_slots_admit_cpu_while_gpu_runs() {
        let slots = worker_slots(1, Some(2), Some(1));
        assert!(slots.admits(CPU, running(0, 1)));
        assert!(!slots.admits(GPU, running(0, 1)));
        assert_eq!(slots.admitted_classes(running(2, 1)), Vec::new());
        assert_eq!(slots.admitted_classes(running(1, 1)), vec![CPU]);
    }

    #[test]
    fn values_are_clamped_and_validated() {
        let slots = worker_slots(99, Some(-4), Some(99));
        assert_eq!(slots.cpu, WORKER_MIN_CLASS_SLOTS);
        assert_eq!(slots.gpu, WORKER_MAX_CLASS_SLOTS);
        assert_eq!(worker_slots(0, None, None).total, WORKER_MIN_CONCURRENCY);
        assert!(class_slots_valid(None, None));
        assert!(class_slots_valid(Some(0), Some(1)));
        assert!(class_slots_valid(None, Some(0)));
        assert!(!class_slots_valid(Some(0), Some(0)));
        assert!(!class_slots_valid(Some(0), None));
        assert!(!class_slots_valid(Some(WORKER_MAX_CLASS_SLOTS + 1), None));
        assert!(!class_slots_valid(None, Some(-1)));
    }

    #[test]
    fn running_jobs_add_by_class() {
        let mut count = RunningJobs::default();
        count.add(GPU, 2);
        count.add(CPU, 1);
        assert_eq!(count, running(1, 2));
        assert_eq!(count.total(), 3);
    }
}
