use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Longueur maximale (en caractères) de la cause d'une indisponibilité exposée à l'administrateur.
const DETAIL_MAX_CHARS: usize = 300;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid input")]
    Invalid,
    #[error("unauthenticated")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("lease expired or conflict")]
    Conflict,
    /// L'appelant n'est plus titulaire du bail : refus définitif (ADR 0027).
    #[error("lease lost")]
    LeaseLost,
    /// Publication inscrite dont l'issue est en cours de vérification : répéter plus tard.
    #[error("publication pending")]
    PublicationPending,
    #[error("source changed since discovery")]
    SourceChanged,
    #[error("installation already completed")]
    AlreadyConfigured,
    #[error("too many attempts")]
    RateLimited,
    #[error("payload too large")]
    TooLarge,
    #[error("checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("tool unavailable: {0}")]
    ToolUnavailable(String),
    #[error("unsupported archive: {0}")]
    UnsupportedArchive(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("invalid flow variables: {0}")]
    InvalidFlowVariables(String),
    #[error("repository unavailable: {0}")]
    RepositoryUnavailable(String),
    #[error("central unavailable")]
    CentralUnavailable,
    #[error("no reachable worker can receive the result")]
    ResultUndeliverable,
    #[error("database failure")]
    Database(sqlx::Error),
    #[error("storage failure")]
    Storage(#[from] std::io::Error),
    #[error("internal failure")]
    Internal,
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        if let Some(db_err) = err.as_database_error()
            && let Some(code) = db_err.code()
        {
            match code.as_ref() {
                "2067" | "1555" => return AppError::Conflict,
                "275" | "787" => return AppError::Invalid,
                _ => {}
            }
        }
        AppError::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_input"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "lease_expired_or_conflict"),
            Self::LeaseLost => (StatusCode::CONFLICT, "lease_lost"),
            Self::PublicationPending => (StatusCode::SERVICE_UNAVAILABLE, "publication_pending"),
            Self::SourceChanged => (StatusCode::CONFLICT, "source_changed"),
            Self::AlreadyConfigured => (StatusCode::CONFLICT, "already_configured"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            Self::ChecksumMismatch { .. }
            | Self::ToolUnavailable(_)
            | Self::UnsupportedArchive(_)
            | Self::InvalidManifest(_) => (StatusCode::BAD_REQUEST, "invalid_tool_artifact"),
            Self::InvalidFlowVariables(_) => (StatusCode::BAD_REQUEST, "invalid_flow_variables"),
            Self::RepositoryUnavailable(_) => (StatusCode::BAD_GATEWAY, "repository_unavailable"),
            Self::CentralUnavailable => (StatusCode::BAD_GATEWAY, "central_unavailable"),
            Self::ResultUndeliverable => (StatusCode::BAD_GATEWAY, "result_undeliverable"),
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "file_too_large"),
            Self::Database(_) | Self::Storage(_) | Self::Internal => {
                tracing::error!(error = ?self, "internal failure");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        let mut error = json!({"code": code});
        if let Self::InvalidFlowVariables(detail) | Self::RepositoryUnavailable(detail) = &self {
            let message = bounded_detail(detail);
            tracing::warn!(code, %message, "détail d'erreur transmis au client");
            error["message"] = json!(message);
        }
        let mut response = (status, Json(json!({"error": error}))).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}

/// Cause sur une seule ligne et bornée : les caractères de contrôle deviennent des espaces.
fn bounded_detail(detail: &str) -> String {
    detail
        .chars()
        .map(visible_character)
        .take(DETAIL_MAX_CHARS)
        .collect()
}

fn visible_character(character: char) -> char {
    if character.is_control() {
        ' '
    } else {
        character
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    const BODY_LIMIT_BYTES: usize = 4096;

    async fn answer(error: AppError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), BODY_LIMIT_BYTES)
            .await
            .unwrap_or_default();
        let body = serde_json::from_slice(&bytes).unwrap_or_default();
        (status, body)
    }

    #[tokio::test]
    async fn repository_unavailable_explains_its_cause_on_one_line() {
        let detail = "réponse HTTP 429\npour https://x.test/index.json";
        let (status, body) = answer(AppError::RepositoryUnavailable(detail.into())).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"]["code"], "repository_unavailable");
        let expected = "réponse HTTP 429 pour https://x.test/index.json";
        assert_eq!(body["error"]["message"], expected);
    }

    #[tokio::test]
    async fn the_cause_of_an_unavailable_repository_is_bounded() {
        let long = "é".repeat(DETAIL_MAX_CHARS * 2);
        let (_, body) = answer(AppError::RepositoryUnavailable(long)).await;
        let message = body["error"]["message"].as_str().unwrap_or_default();
        assert_eq!(message.chars().count(), DETAIL_MAX_CHARS);
    }

    #[tokio::test]
    async fn a_lost_lease_is_told_apart_from_any_other_conflict() {
        let (status, body) = answer(AppError::LeaseLost).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "lease_lost");
        let (status, body) = answer(AppError::Conflict).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "lease_expired_or_conflict");
        let (status, _) = answer(AppError::PublicationPending).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn other_errors_keep_exposing_only_their_code() {
        for error in [AppError::NotFound, AppError::Internal] {
            let (_, body) = answer(error).await;
            assert!(body["error"].get("message").is_none());
            assert!(body["error"]["code"].is_string());
        }
    }
}
