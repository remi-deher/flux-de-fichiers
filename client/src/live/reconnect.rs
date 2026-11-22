//! Réouverture d'un flux que le navigateur a définitivement fermé (erreur d'un proxy, accès perdu).

use super::may_listen;
use crate::api::ApiError;
use forgearr_shared::session::Me;
use std::time::Duration;

const FIRST_RETRY: Duration = Duration::from_secs(2);
const MAX_RETRY: Duration = Duration::from_secs(60);

/// Au-delà, les premières lectures de la page peuvent dater d'avant un changement non reçu.
const FIRST_OPEN_GRACE_MS: f64 = 1_000.0;

/// Faut-il tout relire à l'ouverture ? `first_wait_ms` : attente de la toute première ouverture.
pub fn must_resync(lost: bool, first_wait_ms: Option<f64>) -> bool {
    lost || first_wait_ms.is_some_and(|waited| waited > FIRST_OPEN_GRACE_MS)
}

/// Délai avant l'essai suivant : il double à chaque échec, jusqu'à un plafond.
pub fn retry_delay(failures: u32) -> Duration {
    FIRST_RETRY
        .saturating_mul(2_u32.saturating_pow(failures))
        .min(MAX_RETRY)
}

/// Suite à donner après le contrôle de la session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reopen {
    Now,
    Later,
    Never,
}

impl Reopen {
    /// Un flux ne se rouvre qu'avec une session valide qui peut encore l'écouter.
    pub fn after(identity: &Result<Me, ApiError>) -> Self {
        match identity {
            Ok(me) if may_listen(me) => Self::Now,
            Ok(_) | Err(ApiError::Unauthorized) => Self::Never,
            Err(_) => Self::Later,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::session::CallerKind;
    use forgearr_shared::{InstanceKind, Role};

    #[test]
    fn the_delay_grows_to_its_ceiling() {
        assert_eq!(retry_delay(0), FIRST_RETRY);
        assert!(retry_delay(1) > retry_delay(0));
        assert_eq!(retry_delay(u32::MAX), MAX_RETRY);
    }

    #[test]
    fn a_lost_or_late_first_stream_rereads_everything() {
        assert!(!must_resync(false, None));
        assert!(!must_resync(false, Some(FIRST_OPEN_GRACE_MS)));
        assert!(must_resync(false, Some(FIRST_OPEN_GRACE_MS + 1.0)));
        assert!(must_resync(true, Some(0.0)));
        assert!(must_resync(true, None));
    }

    #[test]
    fn the_stream_reopens_only_with_a_valid_session() {
        let me = |kind| Me::new(CallerKind::Account, "a".into(), Role::Admin, kind, None);
        assert_eq!(Reopen::after(&Ok(me(InstanceKind::Central))), Reopen::Now);
        assert_eq!(Reopen::after(&Ok(me(InstanceKind::Worker))), Reopen::Never);
        assert_eq!(Reopen::after(&Err(ApiError::Unauthorized)), Reopen::Never);
        assert_eq!(Reopen::after(&Err(ApiError::Network)), Reopen::Later);
    }
}
