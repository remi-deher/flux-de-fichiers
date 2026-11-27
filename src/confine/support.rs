//! Ce que le noyau de cette machine sait appliquer (ADR 0034, point 6).

use super::{landlock, seccomp};
use forgearr_shared::confinement::ConfinementSupport;

/// Version de Landlock et disponibilité de seccomp ; un worker la publie avec sa télémétrie.
pub fn detect() -> ConfinementSupport {
    ConfinementSupport {
        landlock_abi: landlock::abi(),
        seccomp: seccomp::available(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_agrees_with_the_building_blocks() {
        let support = detect();
        assert_eq!(support.landlock_abi, landlock::abi());
        assert_eq!(support.seccomp, seccomp::available());
    }
}
