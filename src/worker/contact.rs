//! Contact avec le central (ADR 0027) : sens d'une réponse, dernier contact, patience.

use crate::job_runner::action_gate::StopMemory;
use forgearr_shared::jobs::{UNREACHABLE_TIMEOUT_HEADER, valid_unreachable_timeout};
use reqwest::{StatusCode, header::HeaderMap};
use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

/// Code d'erreur du central : l'appelant n'est plus titulaire du bail.
pub(super) const LEASE_LOST: &str = "lease_lost";
const INTERNAL_ERROR: &str = "internal_error";
const PUBLICATION_PENDING: &str = "publication_pending";

/// Ce que dit une réponse du central.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CentralReply {
    Accepted,
    /// Seule preuve d'un bail perdu.
    LeaseLost,
    /// Refus métier.
    Refused,
    /// Panne interne du central : répétée un nombre borné de fois.
    Broken,
    /// Central injoignable ou momentanément incapable de répondre.
    Unavailable,
}

/// Sens d'une réponse d'après son statut et le code d'erreur de son corps ; sans code, elle ne vient
/// pas du central (proxy).
pub(super) fn classify(status: StatusCode, code: Option<&str>) -> CentralReply {
    if status.is_success() {
        return CentralReply::Accepted;
    }
    match (status, code) {
        (StatusCode::CONFLICT, Some(LEASE_LOST)) => CentralReply::LeaseLost,
        (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS, _) => {
            CentralReply::Unavailable
        }
        // Corps refusé avant d'atteindre la route (trop gros, mal formé) : le répéter ne sert à rien.
        (
            StatusCode::BAD_REQUEST
            | StatusCode::PAYLOAD_TOO_LARGE
            | StatusCode::UNPROCESSABLE_ENTITY,
            _,
        ) => CentralReply::Refused,
        (_, None | Some(PUBLICATION_PENDING)) => CentralReply::Unavailable,
        (_, Some(INTERNAL_ERROR)) => CentralReply::Broken,
        (StatusCode::SERVICE_UNAVAILABLE, _) => CentralReply::Unavailable,
        _ => CentralReply::Refused,
    }
}

/// Code d'erreur du corps d'une réponse du central.
pub(super) fn error_code(body: &[u8]) -> Option<String> {
    let body: serde_json::Value = serde_json::from_slice(body).ok()?;
    body.pointer("/error/code")?.as_str().map(str::to_owned)
}

/// Délai d'injoignabilité rendu par un battement ; absent d'un ancien central.
pub(super) fn announced_timeout(headers: &HeaderMap) -> Option<i64> {
    let value = headers.get(UNREACHABLE_TIMEOUT_HEADER)?;
    value.to_str().ok()?.parse().ok()
}

/// Contact d'une tâche avec le central.
#[derive(Debug)]
pub(super) struct LeaseContact {
    last: Mutex<Instant>,
    /// Délai d'injoignabilité rendu par le dernier battement, s'il en a rendu un.
    unreachable_timeout: Mutex<Option<i64>>,
    /// Arrêt constaté par une demande d'accord.
    pub(super) stop: StopMemory,
}

impl Default for LeaseContact {
    fn default() -> Self {
        Self {
            last: Mutex::new(Instant::now()),
            unreachable_timeout: Mutex::default(),
            stop: StopMemory::default(),
        }
    }
}

impl LeaseContact {
    pub(super) fn touch(&self) {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }

    /// Retient le délai rendu par un battement ; une valeur absente ou hors bornes ne change rien.
    pub(super) fn announce_timeout(&self, seconds: Option<i64>) {
        if let Some(seconds) = seconds.filter(|seconds| valid_unreachable_timeout(*seconds)) {
            *self.timeout_slot() = Some(seconds);
        }
    }

    pub(super) fn announced_timeout(&self) -> Option<i64> {
        *self.timeout_slot()
    }

    fn timeout_slot(&self) -> MutexGuard<'_, Option<i64>> {
        self.unreachable_timeout
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Temps écoulé depuis la dernière réponse du central.
    pub(super) fn silence(&self) -> Duration {
        self.last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lease_lost_proves_the_loss_of_a_lease() {
        let conflict = StatusCode::CONFLICT;
        assert_eq!(
            classify(conflict, Some(LEASE_LOST)),
            CentralReply::LeaseLost
        );
        for code in ["lease_expired_or_conflict", "source_changed"] {
            assert_eq!(classify(conflict, Some(code)), CentralReply::Refused);
        }
        let missing = StatusCode::NOT_FOUND;
        assert_eq!(classify(missing, Some("not_found")), CentralReply::Refused);
        // Un proxy sans central derrière lui répond sans code.
        assert_eq!(classify(missing, None), CentralReply::Unavailable);
        assert_eq!(classify(conflict, None), CentralReply::Unavailable);
        let too_large = StatusCode::PAYLOAD_TOO_LARGE;
        assert_eq!(classify(too_large, None), CentralReply::Refused);
    }

    #[test]
    fn an_unreachable_central_is_not_a_refusal() {
        for (status, code) in [
            (StatusCode::UNAUTHORIZED, Some("unauthenticated")),
            (StatusCode::FORBIDDEN, Some("forbidden")),
            (StatusCode::TOO_MANY_REQUESTS, Some("too_many_attempts")),
            (StatusCode::SERVICE_UNAVAILABLE, Some("publication_pending")),
            (StatusCode::BAD_GATEWAY, None),
            (StatusCode::INTERNAL_SERVER_ERROR, None),
        ] {
            assert_eq!(classify(status, code), CentralReply::Unavailable);
        }
        assert_eq!(
            classify(StatusCode::BAD_GATEWAY, Some("result_undeliverable")),
            CentralReply::Refused
        );
        assert_eq!(
            classify(StatusCode::INTERNAL_SERVER_ERROR, Some(INTERNAL_ERROR)),
            CentralReply::Broken
        );
        assert_eq!(
            classify(StatusCode::NO_CONTENT, None),
            CentralReply::Accepted
        );
    }

    #[test]
    fn only_a_valid_announced_timeout_is_kept() {
        let contact = LeaseContact::default();
        assert_eq!(contact.announced_timeout(), None);
        for (announced, kept) in [(Some(3600), 3600), (None, 3600), (Some(1), 3600)] {
            contact.announce_timeout(announced);
            assert_eq!(contact.announced_timeout(), Some(kept));
        }
        assert_eq!(announced_timeout(&HeaderMap::new()), None);
    }

    #[test]
    fn the_error_code_is_read_from_the_body() {
        let body = br#"{"error":{"code":"lease_lost"}}"#;
        assert_eq!(error_code(body).as_deref(), Some(LEASE_LOST));
        assert_eq!(error_code(b"<html>"), None);
        assert_eq!(error_code(b"{}"), None);
    }
}
