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

/// `POST` d'un corps JSON (ou vide), avec le jeton CSRF ; renvoie la réponse JSON.
pub async fn post<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: Option<&B>,
) -> Result<T, ApiError> {
    let request = Request::post(path).header(CSRF_HEADER, &csrf_token());
    let response = match body {
        Some(body) => {
            let request = request.json(body).map_err(|_| ApiError::Network)?;
            let response = request.send().await.map_err(|_| ApiError::Network)?;
            checked(response).await?
        }
        None => send(request).await?,
    };
    response.json().await.map_err(|_| ApiError::Network)
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
