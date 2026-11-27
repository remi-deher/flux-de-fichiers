//! Plafond global des sessions de langage ouvertes (ADR 0032, point 4) : un processus par éditeur.

use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Sessions ouvertes en même temps, au plus, sauf réglage.
pub const DEFAULT_SESSION_LIMIT: usize = 4;
/// Plus haut réglage accepté : un serveur de langage pèse de 50 à 400 Mio.
pub const MAX_SESSION_LIMIT: usize = 32;
/// Variable d'environnement qui règle le plafond.
pub const SESSION_LIMIT_VARIABLE: &str = "LANGUAGE_SESSIONS_MAX";

#[derive(Clone, Debug)]
pub struct SessionLimiter {
    permits: Arc<Semaphore>,
    limit: usize,
}

impl SessionLimiter {
    pub fn new(limit: usize) -> Self {
        let limit = limit.min(MAX_SESSION_LIMIT);
        Self {
            permits: Arc::new(Semaphore::new(limit)),
            limit,
        }
    }

    /// Plafond donné par `LANGUAGE_SESSIONS_MAX`, sinon `DEFAULT_SESSION_LIMIT`.
    pub fn from_environment() -> Self {
        Self::from_setting(std::env::var(SESSION_LIMIT_VARIABLE).ok().as_deref())
    }

    /// Un réglage absent ou illisible laisse le plafond par défaut ; 0 refuse toute session.
    pub fn from_setting(setting: Option<&str>) -> Self {
        let limit = setting
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(DEFAULT_SESSION_LIMIT);
        Self::new(limit)
    }

    pub const fn limit(&self) -> usize {
        self.limit
    }

    /// Une place de session, rendue quand le permis est lâché ; `None` si le plafond est atteint.
    pub fn try_acquire(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.permits).try_acquire_owned().ok()
    }
}

impl Default for SessionLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_SESSION_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_are_refused_beyond_the_limit_and_places_come_back() {
        let limiter = SessionLimiter::new(2);
        let first = limiter.try_acquire();
        let second = limiter.try_acquire();
        assert!(first.is_some() && second.is_some());
        assert!(limiter.try_acquire().is_none());
        drop(first);
        assert!(limiter.try_acquire().is_some());
    }

    #[test]
    fn clones_share_the_same_places() {
        let limiter = SessionLimiter::new(1);
        let clone = limiter.clone();
        let held = limiter.try_acquire();
        assert!(held.is_some());
        assert!(clone.try_acquire().is_none());
    }

    #[test]
    fn the_setting_is_read_trimmed_and_falls_back_to_the_default() {
        assert_eq!(SessionLimiter::from_setting(Some(" 6 ")).limit(), 6);
        for setting in [None, Some("beaucoup"), Some("-1"), Some("")] {
            let limiter = SessionLimiter::from_setting(setting);
            assert_eq!(limiter.limit(), DEFAULT_SESSION_LIMIT);
        }
    }

    #[test]
    fn zero_refuses_every_session_and_the_limit_is_capped() {
        let none = SessionLimiter::from_setting(Some("0"));
        assert!(none.try_acquire().is_none());
        let huge = SessionLimiter::from_setting(Some("500"));
        assert_eq!(huge.limit(), MAX_SESSION_LIMIT);
    }

    #[test]
    fn the_default_limit_is_four() {
        assert_eq!(SessionLimiter::default().limit(), 4);
    }
}
