//! Enrôlement d'un worker auprès du central : demande à approuver (ADR 0016) et secours par
//! identifiants administrateur (ADR 0011).

use crate::{
    config,
    error::{AppError, AppResult},
    opaque_token::{random_hex, sha256_hex},
};
use forgearr_shared::workers::{
    WorkerEnrollmentCreated, WorkerEnrollmentPollResponse, WorkerEnrollmentRequestInput,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Chemin de la route d'enrôlement direct (secours ADR 0011).
pub const ENROLL_PATH: &str = "/api/v1/workers/enroll";
/// Chemin des demandes d'enrôlement à approuver (ADR 0016).
pub const WORKERS_REQUESTS_PATH: &str = "/api/v1/workers/requests";
/// En-tête portant le secret aléatoire généré par le worker pour retirer sa clé.
pub const ENROLLMENT_SECRET_HEADER: &str = "x-enrollment-secret";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const SECRET_BYTES: usize = 24;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const MAX_REQUESTS_PER_ADDR: usize = 5;
const MAX_TRACKED_ADDRS: usize = 1024;
const WORKER_NAME_MAX_LEN: usize = 80;

static REQUEST_RATE_LIMITER: LazyLock<Mutex<HashMap<String, VecDeque<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

fn build_client() -> AppResult<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    config::trust_ca(builder, config::central_ca().as_deref())
        .and_then(|builder| Ok(builder.build()?))
        .map_err(|error| {
            tracing::error!(error = %error, "client d'enrôlement indisponible");
            AppError::Internal
        })
}

/// Demande au central la clé d'API d'un nouveau worker (voie directe ADR 0011). Aucune redirection
/// n'est suivie et le détail des erreurs réseau n'est pas relayé.
pub async fn request_key(central: &reqwest::Url, request: &EnrollRequest) -> AppResult<String> {
    let client = build_client()?;
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

/// Génère un secret aléatoire côté worker et son empreinte SHA-256 à envoyer au central.
pub fn generate_secret_and_hash() -> (String, String) {
    let secret = random_hex(SECRET_BYTES);
    let hash = sha256_hex(&secret);
    (secret, hash)
}

/// Nom de worker proposé par défaut à partir du nom d'hôte, nettoyé pour respecter les règles.
pub fn default_worker_name() -> String {
    let raw = std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_else(|| "worker".to_owned());
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
        .take(WORKER_NAME_MAX_LEN)
        .collect();
    if cleaned.is_empty() {
        "worker".to_owned()
    } else {
        cleaned
    }
}

/// Limite la fréquence des demandes d'enrôlement par adresse source.
pub fn check_request_rate_limit(addr: Option<&str>) -> AppResult<()> {
    let key = addr
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");
    let now = Instant::now();
    let mut guard = REQUEST_RATE_LIMITER
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if guard.len() >= MAX_TRACKED_ADDRS && !guard.contains_key(key) {
        guard.retain(|_, hits| {
            hits.front()
                .is_some_and(|first| now.duration_since(*first) < RATE_WINDOW)
        });
        if guard.len() >= MAX_TRACKED_ADDRS {
            return Err(AppError::RateLimited);
        }
    }
    let hits = guard.entry(key.to_owned()).or_default();
    while hits
        .front()
        .is_some_and(|ts| now.duration_since(*ts) >= RATE_WINDOW)
    {
        hits.pop_front();
    }
    if hits.len() >= MAX_REQUESTS_PER_ADDR {
        return Err(AppError::RateLimited);
    }
    hits.push_back(now);
    Ok(())
}

/// Dépose une demande d'enrôlement auprès du central (ADR 0016).
pub async fn submit_enrollment_request(
    central: &reqwest::Url,
    request: &WorkerEnrollmentRequestInput,
) -> AppResult<WorkerEnrollmentCreated> {
    let client = build_client()?;
    let url = central
        .join(WORKERS_REQUESTS_PATH)
        .map_err(|_| AppError::Invalid)?;
    let response = client
        .post(url)
        .json(request)
        .send()
        .await
        .map_err(|_| AppError::CentralUnavailable)?;
    if let Some(error) = error_for_status(response.status()) {
        return Err(error);
    }
    response
        .json::<WorkerEnrollmentCreated>()
        .await
        .map_err(|_| AppError::CentralUnavailable)
}

/// Interroge le central sur l'avancement d'une demande et récupère la clé si approuvée.
pub async fn poll_enrollment_request(
    central: &reqwest::Url,
    id: Uuid,
    secret: &str,
) -> AppResult<WorkerEnrollmentPollResponse> {
    let client = build_client()?;
    let path = format!("{WORKERS_REQUESTS_PATH}/{id}");
    let url = central.join(&path).map_err(|_| AppError::Invalid)?;
    let response = client
        .get(url)
        .header(ENROLLMENT_SECRET_HEADER, secret)
        .send()
        .await
        .map_err(|_| AppError::CentralUnavailable)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    if let Some(error) = error_for_status(response.status()) {
        return Err(error);
    }
    response
        .json::<WorkerEnrollmentPollResponse>()
        .await
        .map_err(|_| AppError::CentralUnavailable)
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
