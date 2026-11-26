//! Plafonds de ressources par `setrlimit` (ADR 0034, point 4).

use super::{ConfineError, last_os_error};
use forgearr_shared::confinement::ResourceLimits;

#[cfg(target_env = "gnu")]
type Resource = libc::__rlimit_resource_t;
#[cfg(not(target_env = "gnu"))]
type Resource = libc::c_int;

const NO_CORE_DUMP: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LimitKind {
    CoreDump,
    /// Données du processus (pas l'espace d'adressage : V8 en réserve beaucoup sans l'utiliser).
    Data,
    FileSize,
    CpuTime,
    OpenFiles,
}

impl LimitKind {
    fn resource(self) -> Resource {
        match self {
            Self::CoreDump => libc::RLIMIT_CORE,
            Self::Data => libc::RLIMIT_DATA,
            Self::FileSize => libc::RLIMIT_FSIZE,
            Self::CpuTime => libc::RLIMIT_CPU,
            Self::OpenFiles => libc::RLIMIT_NOFILE,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CoreDump => "setrlimit(core)",
            Self::Data => "setrlimit(data)",
            Self::FileSize => "setrlimit(fsize)",
            Self::CpuTime => "setrlimit(cpu)",
            Self::OpenFiles => "setrlimit(nofile)",
        }
    }
}

/// Les plafonds à poser : aucun fichier de vidage mémoire, plus ceux que le profil demande.
fn planned_limits(limits: &ResourceLimits) -> Vec<(LimitKind, u64)> {
    let mut plan = vec![(LimitKind::CoreDump, NO_CORE_DUMP)];
    let requested = [
        (LimitKind::Data, limits.memory_bytes),
        (LimitKind::FileSize, limits.max_file_bytes),
        (LimitKind::CpuTime, limits.cpu_seconds),
        (LimitKind::OpenFiles, limits.open_files),
    ];
    for (kind, value) in requested {
        if let Some(value) = value {
            plan.push((kind, value));
        }
    }
    plan
}

/// Un processus ne peut pas relever sa limite stricte : le plafond demandé est ramené à celle en place.
fn clamp_to_hard_limit(requested: u64, hard_limit: u64) -> u64 {
    requested.min(hard_limit)
}

fn set_limit(kind: LimitKind, requested: u64) -> Result<(), ConfineError> {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `current` est une structure `rlimit` valide et inscriptible pendant l'appel.
    if unsafe { libc::getrlimit(kind.resource(), &raw mut current) } != 0 {
        return Err(last_os_error(kind.label()));
    }
    let value = clamp_to_hard_limit(requested, current.rlim_max);
    let limit = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    // SAFETY: `limit` est une structure `rlimit` valide pendant l'appel, qui la copie.
    if unsafe { libc::setrlimit(kind.resource(), &raw const limit) } != 0 {
        return Err(last_os_error(kind.label()));
    }
    Ok(())
}

/// Pose les plafonds sur le processus courant (hérités par ses enfants).
pub fn apply(limits: &ResourceLimits) -> Result<(), ConfineError> {
    for (kind, value) in planned_limits(limits) {
        set_limit(kind, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_dumps_are_always_disabled() {
        let unlimited = ResourceLimits {
            memory_bytes: None,
            max_file_bytes: None,
            cpu_seconds: None,
            open_files: None,
        };
        assert_eq!(planned_limits(&unlimited), vec![(LimitKind::CoreDump, 0)]);
    }

    #[test]
    fn the_plan_follows_the_requested_limits_in_order() {
        let limits = ResourceLimits {
            memory_bytes: Some(10),
            max_file_bytes: None,
            cpu_seconds: Some(30),
            open_files: Some(40),
        };
        assert_eq!(
            planned_limits(&limits),
            vec![
                (LimitKind::CoreDump, 0),
                (LimitKind::Data, 10),
                (LimitKind::CpuTime, 30),
                (LimitKind::OpenFiles, 40),
            ]
        );
    }

    #[test]
    fn a_limit_never_exceeds_the_hard_limit_in_place() {
        assert_eq!(clamp_to_hard_limit(100, 50), 50);
        assert_eq!(clamp_to_hard_limit(10, 50), 10);
        assert_eq!(clamp_to_hard_limit(10, u64::MAX), 10);
    }

    #[test]
    fn every_kind_has_its_own_resource_and_label() {
        let kinds = [
            LimitKind::CoreDump,
            LimitKind::Data,
            LimitKind::FileSize,
            LimitKind::CpuTime,
            LimitKind::OpenFiles,
        ];
        let mut resources: Vec<_> = kinds.iter().map(|kind| kind.resource()).collect();
        resources.sort_unstable();
        resources.dedup();
        assert_eq!(resources.len(), kinds.len());
        let labels: Vec<_> = kinds.iter().map(|kind| kind.label()).collect();
        assert!(labels.iter().all(|label| label.starts_with("setrlimit(")));
    }

    #[test]
    fn a_limit_equal_to_the_current_one_is_accepted() -> Result<(), ConfineError> {
        let mut current = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `current` est une structure `rlimit` valide et inscriptible pendant l'appel.
        let read = unsafe { libc::getrlimit(LimitKind::CoreDump.resource(), &raw mut current) };
        assert_eq!(read, 0);
        set_limit(LimitKind::CoreDump, current.rlim_max)
    }
}
