use crate::error::{AppError, AppResult};
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub fn authorize(headers: &HeaderMap, key: &str) -> AppResult<()> {
    let actual = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    let matches = Sha256::digest(actual.as_bytes()).ct_eq(&Sha256::digest(key.as_bytes()));
    if bool::from(matches) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}
