//! Appels typés à l'API du central, avec la session cookie et son jeton CSRF.

use gloo_net::http::{Request, RequestBuilder, Response};
use serde::{Serialize, de::DeserializeOwned};
use std::cell::RefCell;

/// En-tête CSRF exigé par le central sur toute requête qui modifie.
const CSRF_HEADER: &str = "x-csrf-token";

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

/// Échec d'un appel, tel que l'interface doit le présenter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiError {
    /// Pas de session : il faut se connecter.
    Unauthorized,
    /// Refus ou erreur du central, avec son code d'erreur stable.
    Rejected { status: u16, code: String },
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
}

async fn checked(response: Response) -> Result<Response, ApiError> {
    match response.status() {
        200..=299 => Ok(response),
        401 => Err(ApiError::Unauthorized),
        status => {
            let code = response
                .json::<ErrorBody>()
                .await
                .map(|body| body.error.code)
                .unwrap_or_else(|_| "unknown".into());
            Err(ApiError::Rejected { status, code })
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
    fn errors_have_a_readable_message() {
        assert!(ApiError::Unauthorized.message().contains("reconnectez"));
        let conflict = ApiError::Rejected {
            status: 409,
            code: "conflict".into(),
        };
        assert!(conflict.message().contains("conflit"));
        let other = ApiError::Rejected {
            status: 422,
            code: "invalid_input".into(),
        };
        assert!(other.message().contains("invalid_input"));
    }

    #[test]
    fn the_csrf_token_is_kept_for_later_requests() {
        set_csrf_token("abc");
        assert_eq!(csrf_token(), "abc");
    }
}
