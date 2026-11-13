//! Enrôlement d'un worker auprès du central (ADR 0011) : le worker présente les identifiants d'un
//! administrateur, le central crée la clé d'API du worker et la renvoie une seule fois. Le mot de passe
//! n'est jamais conservé par le worker.

use crate::error::{AppError, AppResult};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Chemin de la route d'enrôlement, partagé entre le client (worker) et le serveur (central).
pub const ENROLL_PATH: &str = "/api/v1/workers/enroll";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize, Deserialize)]
pub struct EnrollRequest {
    pub username: String,
    pub password: String,
    pub worker_name: String,
}

/// Réponse du central : `key` n'existe qu'ici.
#[derive(Serialize, Deserialize)]
pub struct EnrolledWorker {
    pub key: String,
}

/// Traduit le statut renvoyé par le central en erreur applicative pour l'interface.
pub fn error_for_status(status: StatusCode) -> Option<AppError> {
    match status {
        status if status.is_success() => None,
        StatusCode::BAD_REQUEST => Some(AppError::Invalid),
        StatusCode::UNAUTHORIZED => Some(AppError::Unauthorized),
        StatusCode::FORBIDDEN => Some(AppError::Forbidden),
        StatusCode::TOO_MANY_REQUESTS => Some(AppError::RateLimited),
        _ => Some(AppError::CentralUnavailable),
    }
}

/// Demande au central la clé d'API d'un nouveau worker. Aucune redirection n'est suivie et le détail
/// des erreurs réseau n'est pas relayé (l'adresse saisie ne doit pas fuiter dans les messages).
pub async fn request_key(central: &reqwest::Url, request: &EnrollRequest) -> AppResult<String> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::Internal)?;
    let url = central.join(ENROLL_PATH).map_err(|_| AppError::Invalid)?;
    let response = client
        .post(url)
        .json(request)
        .send()
        .await
        .map_err(|_| AppError::CentralUnavailable)?;
    if let Some(error) = error_for_status(response.status()) {
        return Err(error);
    }
    let enrolled: EnrolledWorker = response
        .json()
        .await
        .map_err(|_| AppError::CentralUnavailable)?;
    Ok(enrolled.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_central_statuses_map_to_actionable_errors() {
        assert!(error_for_status(StatusCode::OK).is_none());
        assert!(matches!(
            error_for_status(StatusCode::UNAUTHORIZED),
            Some(AppError::Unauthorized)
        ));
        assert!(matches!(
            error_for_status(StatusCode::FORBIDDEN),
            Some(AppError::Forbidden)
        ));
        assert!(matches!(
            error_for_status(StatusCode::TOO_MANY_REQUESTS),
            Some(AppError::RateLimited)
        ));
        assert!(matches!(
            error_for_status(StatusCode::BAD_REQUEST),
            Some(AppError::Invalid)
        ));
        assert!(matches!(
            error_for_status(StatusCode::NOT_FOUND),
            Some(AppError::CentralUnavailable)
        ));
        assert!(matches!(
            error_for_status(StatusCode::INTERNAL_SERVER_ERROR),
            Some(AppError::CentralUnavailable)
        ));
    }

    #[tokio::test]
    async fn test_unreachable_central_is_reported_without_details() -> anyhow::Result<()> {
        let url = reqwest::Url::parse("http://127.0.0.1:9")?;
        let result = request_key(
            &url,
            &EnrollRequest {
                username: "admin".into(),
                password: "x".into(),
                worker_name: "nas".into(),
            },
        )
        .await;
        assert!(matches!(result, Err(AppError::CentralUnavailable)));
        Ok(())
    }
}
