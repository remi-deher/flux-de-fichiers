//! Authentification : un principal est un compte (session web) ou une clé d'API (en-tête Bearer).

use crate::{
    api_keys,
    config::Config,
    error::{AppError, AppResult},
    repository, web_session,
};
use axum::http::{HeaderMap, Method, header};
use flux_shared::{Permission, Role};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalKind {
    Account,
    ApiKey,
}

/// Qui parle : un compte connecté ou une clé d'API.
#[derive(Clone, Debug)]
pub struct Principal {
    pub kind: PrincipalKind,
    pub id: String,
    pub name: String,
    pub role: Role,
    /// Empreinte de la session web, pour l'exclure d'une fermeture de sessions.
    pub session_hash: Option<String>,
}

impl Principal {
    pub fn allows(&self, permission: Permission) -> bool {
        self.role.allows(permission)
    }
}

pub fn require(principal: &Principal, permission: Permission) -> AppResult<()> {
    if principal.allows(permission) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

async fn authenticate_bearer(pool: &SqlitePool, headers: &HeaderMap) -> AppResult<Principal> {
    let token = bearer_token(headers).ok_or(AppError::Unauthorized)?;
    let record = api_keys::authenticate(pool, token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(Principal {
        kind: PrincipalKind::ApiKey,
        id: record.id,
        name: record.name,
        role: Role::parse(&record.role).ok_or(AppError::Internal)?,
        session_hash: None,
    })
}

/// Identifie l'appelant. Un en-tête `Authorization` impose la clé d'API, sans repli sur un cookie
/// ambiant s'il est invalide ; sinon la session web est exigée, avec CSRF et origine si la requête modifie.
pub async fn authenticate(
    pool: &SqlitePool,
    config: &Config,
    headers: &HeaderMap,
    method: &Method,
) -> AppResult<Principal> {
    if headers.contains_key(header::AUTHORIZATION) {
        return authenticate_bearer(pool, headers).await;
    }
    let session = web_session::from_headers(pool, headers).await?;
    if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        web_session::validate_origin(headers, config)?;
        web_session::validate_csrf(headers, &session)?;
    }
    let (account, _) = repository::get_account(pool, &session.account_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if account.disabled {
        return Err(AppError::Unauthorized);
    }
    Ok(Principal {
        kind: PrincipalKind::Account,
        id: account.id,
        name: account.username,
        role: account.role,
        session_hash: Some(session.token_hash),
    })
}

/// Clé d'API d'un worker : protocole de worker seulement.
pub async fn authorize_worker(pool: &SqlitePool, headers: &HeaderMap) -> AppResult<Principal> {
    let principal = authenticate_bearer(pool, headers).await?;
    require(&principal, Permission::WorkerProtocol)?;
    Ok(principal)
}

/// Échecs consécutifs tolérés avant un blocage temporaire.
const MAX_FAILURES: u32 = 5;
const LOCKOUT_BASE: Duration = Duration::from_secs(30);
const LOCKOUT_MAX: Duration = Duration::from_secs(15 * 60);
/// Un compteur inactif depuis ce délai est oublié.
const FAILURE_MEMORY: Duration = Duration::from_secs(60 * 60);

struct Failures {
    count: u32,
    last: Instant,
}

static LOGIN_FAILURES: LazyLock<Mutex<HashMap<String, Failures>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn lockout_for(count: u32) -> Duration {
    let doublings = count.saturating_sub(MAX_FAILURES).min(16);
    LOCKOUT_BASE
        .saturating_mul(1u32 << doublings)
        .min(LOCKOUT_MAX)
}

/// Refuse la tentative si cet identifiant est temporairement bloqué.
pub fn check_login_allowed(username: &str) -> AppResult<()> {
    let key = username.to_lowercase();
    let mut failures = LOGIN_FAILURES.lock().map_err(|_| AppError::Internal)?;
    failures.retain(|_, entry| entry.last.elapsed() < FAILURE_MEMORY);
    match failures.get(&key) {
        Some(entry)
            if entry.count >= MAX_FAILURES && entry.last.elapsed() < lockout_for(entry.count) =>
        {
            Err(AppError::RateLimited)
        }
        _ => Ok(()),
    }
}

pub fn record_login_failure(username: &str) {
    if let Ok(mut failures) = LOGIN_FAILURES.lock() {
        let entry = failures.entry(username.to_lowercase()).or_insert(Failures {
            count: 0,
            last: Instant::now(),
        });
        entry.count += 1;
        entry.last = Instant::now();
    }
}

pub fn record_login_success(username: &str) {
    if let Ok(mut failures) = LOGIN_FAILURES.lock() {
        failures.remove(&username.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockout_grows_then_is_capped() {
        assert_eq!(lockout_for(MAX_FAILURES), LOCKOUT_BASE);
        assert_eq!(lockout_for(MAX_FAILURES + 1), LOCKOUT_BASE * 2);
        assert_eq!(lockout_for(MAX_FAILURES + 40), LOCKOUT_MAX);
    }

    #[test]
    fn test_failures_block_after_the_threshold_and_success_clears_them() {
        let user = "limiter-test-user";
        assert!(check_login_allowed(user).is_ok());
        for _ in 0..MAX_FAILURES {
            record_login_failure(user);
        }
        assert!(matches!(
            check_login_allowed(user),
            Err(AppError::RateLimited)
        ));
        assert!(matches!(
            check_login_allowed("LIMITER-TEST-USER"),
            Err(AppError::RateLimited)
        ));
        record_login_success(user);
        assert!(check_login_allowed(user).is_ok());
    }

    #[test]
    fn test_bearer_token_is_read_only_from_a_bearer_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer abc"
                .parse()
                .unwrap_or_else(|_| header::HeaderValue::from_static("")),
        );
        assert_eq!(bearer_token(&headers), Some("abc"));
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_static("Basic abc"),
        );
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn test_require_checks_the_role() {
        let admin = Principal {
            kind: PrincipalKind::Account,
            id: "1".into(),
            name: "admin".into(),
            role: Role::Admin,
            session_hash: None,
        };
        let worker = Principal {
            role: Role::Worker,
            ..admin.clone()
        };
        assert!(require(&admin, Permission::ManageApiKeys).is_ok());
        assert!(matches!(
            require(&worker, Permission::ManageApiKeys),
            Err(AppError::Forbidden)
        ));
        assert!(require(&worker, Permission::WorkerProtocol).is_ok());
    }
}
