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
    #[error("checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("tool unavailable: {0}")]
    ToolUnavailable(String),
    #[error("unsupported archive: {0}")]
    UnsupportedArchive(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("repository unavailable: {0}")]
    RepositoryUnavailable(String),
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
            Self::ChecksumMismatch { .. }
            | Self::ToolUnavailable(_)
            | Self::UnsupportedArchive(_)
            | Self::InvalidManifest(_) => (StatusCode::BAD_REQUEST, "invalid_tool_artifact"),
            Self::RepositoryUnavailable(_) => (StatusCode::BAD_GATEWAY, "repository_unavailable"),
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "file_too_large"),
            Self::Database(_) | Self::Storage(_) | Self::Internal => {
                tracing::error!(error = ?self, "internal failure");
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
