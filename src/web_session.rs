use crate::{
    auth,
    config::Config,
    error::{AppError, AppResult},
};
use axum::http::{HeaderMap, Method, header};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "flux_session";
pub const CSRF_COOKIE: &str = "flux_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";

#[derive(Clone, Debug)]
pub struct WebSession {
    pub token: String,
    pub csrf_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct ValidatedSession {
    pub token_hash: String,
    pub csrf_hash: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn create(pool: &SqlitePool, ttl_seconds: i64) -> AppResult<WebSession> {
    let token = Uuid::new_v4().to_string();
    let csrf_token = Uuid::new_v4().to_string();
    let token_hash = digest(&token);
    let csrf_hash = digest(&csrf_token);
    let expires_at = Utc::now() + Duration::seconds(ttl_seconds);
    sqlx::query("DELETE FROM sessions WHERE datetime(expires_at) <= datetime('now')")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO sessions(token_hash, expires_at, csrf_hash) VALUES (?1, ?2, ?3)")
        .bind(&token_hash)
        .bind(expires_at)
        .bind(&csrf_hash)
        .execute(pool)
        .await?;
    Ok(WebSession {
        token,
        csrf_token,
        expires_at,
    })
}

pub fn validate_origin(headers: &HeaderMap, config: &Config) -> AppResult<()> {
    let origin = single_header(headers, header::ORIGIN.as_str()).ok_or(AppError::Forbidden)?;
    if let Some(ref configured_origin) = config.web_origin {
        if bool::from(origin.as_bytes().ct_eq(configured_origin.as_bytes())) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    } else {
        let origin_url = reqwest::Url::parse(origin).map_err(|_| AppError::Forbidden)?;
        if origin_url.scheme() != "http" && origin_url.scheme() != "https" {
            return Err(AppError::Forbidden);
        }
        let host_header =
            single_header(headers, header::HOST.as_str()).ok_or(AppError::Forbidden)?;
        let origin_host = match origin_url.port() {
            Some(port) => format!("{}:{}", origin_url.host_str().unwrap_or(""), port),
            None => origin_url.host_str().unwrap_or("").to_string(),
        };
        let matches = bool::from(origin_host.as_bytes().ct_eq(host_header.as_bytes()))
            || (origin_url.port().is_none() && {
                let default_port = match origin_url.scheme() {
                    "https" => ":443",
                    "http" => ":80",
                    _ => "",
                };
                let with_port = format!("{}{default_port}", origin_url.host_str().unwrap_or(""));
                bool::from(with_port.as_bytes().ct_eq(host_header.as_bytes()))
            });
        if matches {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

pub async fn from_headers(pool: &SqlitePool, headers: &HeaderMap) -> AppResult<ValidatedSession> {
    let token = cookie(headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
    let token_hash = digest(token);
    let row: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT csrf_hash, expires_at FROM sessions WHERE token_hash=?1 AND csrf_hash IS NOT NULL AND datetime(expires_at) > datetime('now')",
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?;
    let (csrf_hash, expires_at) = row.ok_or(AppError::Unauthorized)?;
    Ok(ValidatedSession {
        token_hash,
        csrf_hash,
        expires_at,
    })
}

pub fn validate_csrf(headers: &HeaderMap, session: &ValidatedSession) -> AppResult<String> {
    let cookie_token = validate_csrf_cookie(headers, session)?;
    let header_token = single_header(headers, CSRF_HEADER).ok_or(AppError::Forbidden)?;
    let header_matches = header_token.as_bytes().ct_eq(cookie_token.as_bytes());
    if bool::from(header_matches) {
        Ok(cookie_token.to_owned())
    } else {
        Err(AppError::Forbidden)
    }
}

pub fn validate_csrf_cookie<'a>(
    headers: &'a HeaderMap,
    session: &ValidatedSession,
) -> AppResult<&'a str> {
    let cookie_token = cookie(headers, CSRF_COOKIE).ok_or(AppError::Forbidden)?;
    let cookie_hash = digest(cookie_token);
    let cookie_matches = cookie_hash.as_bytes().ct_eq(session.csrf_hash.as_bytes());
    if bool::from(cookie_matches) {
        Ok(cookie_token)
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn authorize_admin(
    pool: &SqlitePool,
    config: &Config,
    headers: &HeaderMap,
    method: &Method,
) -> AppResult<()> {
    // An explicit Authorization header selects Bearer auth; do not fall back to
    // ambient cookies when that credential is malformed or invalid.
    if headers.contains_key(header::AUTHORIZATION) {
        return auth::authorize(headers, &config.admin_key);
    }
    let session = from_headers(pool, headers).await?;
    if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        validate_origin(headers, config)?;
        validate_csrf(headers, &session)?;
    }
    Ok(())
}

pub async fn revoke(pool: &SqlitePool, session: &ValidatedSession) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash=?1")
        .bind(&session.token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn session_cookie(session: &WebSession, config: &Config) -> String {
    cookie_header(SESSION_COOKIE, &session.token, session, config, true)
}

pub fn csrf_cookie(session: &WebSession, config: &Config) -> String {
    cookie_header(CSRF_COOKIE, &session.csrf_token, session, config, false)
}

pub fn clear_cookies(config: &Config) -> [String; 2] {
    let secure = secure_attribute(config);
    [
        format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict{secure}"),
        format!("{CSRF_COOKIE}=; Path=/; Max-Age=0; SameSite=Strict{secure}"),
    ]
}

fn cookie_header(
    name: &str,
    value: &str,
    session: &WebSession,
    config: &Config,
    http_only: bool,
) -> String {
    let http_only = if http_only { "; HttpOnly" } else { "" };
    let secure = secure_attribute(config);
    let max_age = config.web_session_ttl_seconds;
    format!(
        "{name}={value}; Path=/; Max-Age={max_age}; Expires={}; SameSite=Strict{http_only}{secure}",
        session.expires_at.format("%a, %d %b %Y %H:%M:%S GMT")
    )
}

fn secure_attribute(config: &Config) -> &'static str {
    if let Some(ref origin) = config.web_origin {
        if origin.starts_with("https://") {
            return "; Secure";
        }
    }
    ""
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut found = None;
    for header_value in headers.get_all(header::COOKIE).iter() {
        for part in header_value.to_str().ok()?.split(';') {
            let Some((key, value)) = part.trim().split_once('=') else {
                continue;
            };
            if key.trim() == name {
                if found.is_some() {
                    return None;
                }
                found = Some(value.trim());
            }
        }
    }
    found
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value.to_str().ok()
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
