use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

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
    #[error("payload too large")]
    TooLarge,
    #[error("database failure")]
    Database(#[from] sqlx::Error),
    #[error("storage failure")]
    Storage(#[from] std::io::Error),
    #[error("internal failure")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_input"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "lease_expired_or_conflict"),
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "file_too_large"),
            Self::Database(_) | Self::Storage(_) | Self::Internal => {
                // Never log database URLs, SQL errors or filesystem paths supplied by users.
                tracing::error!(error_kind = %self, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        let mut response = (status, Json(json!({"error": {"code": code}}))).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;
