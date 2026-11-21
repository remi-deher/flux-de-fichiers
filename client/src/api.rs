//! Appels typés à l'API du central, avec la session cookie et son jeton CSRF.

use gloo_net::http::{Request, RequestBuilder, Response};
use serde::{Serialize, de::DeserializeOwned};
use std::cell::RefCell;

/// En-tête CSRF exigé par le central sur toute requête qui modifie.
const CSRF_HEADER: &str = "x-csrf-token";
const UNAUTHORIZED_STATUS: u16 = 401;

thread_local! {
    static CSRF_TOKEN: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Retient le jeton CSRF de la session ouverte.
pub fn set_csrf_token(token: &str) {
    CSRF_TOKEN.with(|cell| token.clone_into(&mut cell.borrow_mut()));
}

fn csrf_token() -> String {
    CSRF_TOKEN.with(|cell| cell.borrow().clone())
}

/// Une session ouverte est-elle connue ? (le jeton CSRF ne vient qu'avec une session)
fn session_active() -> bool {
    CSRF_TOKEN.with(|cell| !cell.borrow().is_empty())
}

/// Un 401 sur une session ouverte signifie qu'elle a expiré : retour à l'écran de connexion.
fn is_expired_session(status: u16, session_active: bool) -> bool {
    status == UNAUTHORIZED_STATUS && session_active
}

/// Oublie la session expirée puis recharge la page, qui affiche alors la connexion.
fn restart_login() {
    CSRF_TOKEN.with(|cell| cell.borrow_mut().clear());
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// Échec d'un appel, tel que l'interface doit le présenter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiError {
    /// Pas de session : il faut se connecter.
    Unauthorized,
    /// Refus ou erreur du central : code d'erreur stable et, s'il en donne une, la cause.
    Rejected {
        status: u16,
        code: String,
        detail: Option<String>,
    },
    /// Central injoignable ou réponse illisible.
    Network,
}

impl ApiError {
    /// Code d'erreur stable du central, s'il a répondu.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Rejected { code, .. } => Some(code),
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unauthorized => "Session absente ou expirée : reconnectez-vous.".into(),
            Self::Rejected { status: 409, .. } => {
                "Action refusée dans l'état actuel (conflit).".into()
            }
            Self::Rejected { status: 403, .. } => "Droit insuffisant pour cette action.".into(),
            Self::Rejected { status: 429, .. } => {
                "Trop de demandes ou quota quotidien atteint : réessayez plus tard.".into()
            }
            Self::Rejected {
                code,
                detail: Some(detail),
                ..
            } => format!("Refusé par le central ({code}) : {detail}."),
            Self::Rejected { code, .. } => format!("Refusé par le central ({code})."),
            Self::Network => "Central injoignable ou réponse illisible.".into(),
        }
    }
}

#[derive(serde::Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(serde::Deserialize)]
struct ErrorDetail {
    code: String,
    #[serde(default)]
    message: Option<String>,
}

async fn checked(response: Response) -> Result<Response, ApiError> {
    if is_expired_session(response.status(), session_active()) {
        restart_login();
    }
    match response.status() {
        200..=299 => Ok(response),
        401 => Err(ApiError::Unauthorized),
        status => {
            let body = response.json::<ErrorBody>().await;
            let (code, detail) = match body {
                Ok(body) => (body.error.code, body.error.message),
                Err(_) => ("unknown".into(), None),
            };
            Err(ApiError::Rejected {
                status,
                code,
                detail,
            })
        }
    }
}

async fn send(request: RequestBuilder) -> Result<Response, ApiError> {
    let response = request.send().await.map_err(|_| ApiError::Network)?;
    checked(response).await
}

/// `GET` d'un document JSON.
pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    send(Request::get(path))
        .await?
        .json()
        .await
        .map_err(|_| ApiError::Network)
}

/// Méthode d'une requête qui modifie : elle porte le jeton CSRF.
#[derive(Clone, Copy)]
enum Write {
    Post,
    Put,
    Delete,
}

fn write_request(method: Write, path: &str) -> RequestBuilder {
    let builder = match method {
        Write::Post => Request::post(path),
        Write::Put => Request::put(path),
        Write::Delete => Request::delete(path),
    };
    builder.header(CSRF_HEADER, &csrf_token())
}

async fn send_write<B: Serialize>(
    method: Write,
    path: &str,
    body: Option<&B>,
) -> Result<Response, ApiError> {
    let request = write_request(method, path);
    match body {
        Some(body) => {
            let request = request.json(body).map_err(|_| ApiError::Network)?;
            let response = request.send().await.map_err(|_| ApiError::Network)?;
            checked(response).await
        }
        None => send(request).await,
    }
}

/// `POST` d'un corps JSON (ou vide) ; renvoie la réponse JSON.
pub async fn post<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: Option<&B>,
) -> Result<T, ApiError> {
    send_write(Write::Post, path, body)
        .await?
        .json()
        .await
        .map_err(|_| ApiError::Network)
}

/// `PUT` d'un corps JSON ; renvoie la réponse JSON.
pub async fn put<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, ApiError> {
    send_write(Write::Put, path, Some(body))
        .await?
        .json()
        .await
        .map_err(|_| ApiError::Network)
}

/// `PUT` d'un corps JSON ; renvoie aussi la valeur éventuelle d'un en-tête de réponse.
pub async fn put_with_header<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: &B,
    header_name: &str,
) -> Result<(T, Option<String>), ApiError> {
    let response = send_write(Write::Put, path, Some(body)).await?;
    let header_val = response
        .headers()
        .get(header_name)
        .filter(|val| !val.trim().is_empty());
    let data = response.json().await.map_err(|_| ApiError::Network)?;
    Ok((data, header_val))
}

/// `POST` ou `PUT` dont la réponse n'a pas de corps (204).
pub async fn post_empty<B: Serialize>(path: &str, body: Option<&B>) -> Result<(), ApiError> {
    send_write(Write::Post, path, body).await.map(|_| ())
}

pub async fn put_empty<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    send_write(Write::Put, path, Some(body)).await.map(|_| ())
}

/// `DELETE` sans corps ni réponse.
pub async fn delete(path: &str) -> Result<(), ApiError> {
    send_write::<()>(Write::Delete, path, None)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_401_on_an_open_session_restarts_the_login() {
        assert!(is_expired_session(401, true));
        assert!(!is_expired_session(401, false));
        assert!(!is_expired_session(403, true));
        assert!(!is_expired_session(200, true));
    }

    #[test]
    fn errors_have_a_readable_message() {
        assert!(ApiError::Unauthorized.message().contains("reconnectez"));
        let conflict = ApiError::Rejected {
            status: 409,
            code: "conflict".into(),
            detail: None,
        };
        assert!(conflict.message().contains("conflit"));
        let other = ApiError::Rejected {
            status: 422,
            code: "invalid_input".into(),
            detail: None,
        };
        assert!(other.message().contains("invalid_input"));
        let limited = ApiError::Rejected {
            status: 429,
            code: "too_many_attempts".into(),
            detail: None,
        };
        assert!(limited.message().contains("quota"));
    }

    #[test]
    fn a_rejection_shows_the_cause_the_central_gives() {
        let unavailable = ApiError::Rejected {
            status: 502,
            code: "repository_unavailable".into(),
            detail: Some("réponse HTTP 429 pour https://x.test/index.json".into()),
        };
        let message = unavailable.message();
        assert!(message.contains("repository_unavailable"));
        assert!(message.contains("réponse HTTP 429"));
        let bare = ApiError::Rejected {
            status: 502,
            code: "repository_unavailable".into(),
            detail: None,
        };
        assert_eq!(
            bare.message(),
            "Refusé par le central (repository_unavailable)."
        );
    }

    #[test]
    fn error_bodies_may_carry_a_cause_or_not() -> Result<(), serde_json::Error> {
        let with_cause = r#"{"error":{"code":"repository_unavailable","message":"coupé"}}"#;
        let body: ErrorBody = serde_json::from_str(with_cause)?;
        assert_eq!(body.error.message.as_deref(), Some("coupé"));
        let without = r#"{"error":{"code":"not_found"}}"#;
        let body: ErrorBody = serde_json::from_str(without)?;
        assert_eq!(body.error.code, "not_found");
        assert_eq!(body.error.message, None);
        Ok(())
    }

    #[test]
    fn the_csrf_token_is_kept_for_later_requests() {
        set_csrf_token("abc");
        assert_eq!(csrf_token(), "abc");
    }
}
