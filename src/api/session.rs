//! Connexion, session web, identité de l'appelant et changement de mot de passe.

use super::{AppState, auth::AnyAuth};
use crate::{
    auth::{self, PrincipalKind},
    error::{AppError, AppResult},
    password, repository,
    web_session::{self, WebSession},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use flux_shared::NodeKind;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct SessionResponse {
    csrf_token: String,
    expires_at: String,
    username: String,
}

/// Identité de l'appelant et ce qu'il peut faire, pour que l'interface n'affiche que le permis.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    kind: &'static str,
    name: String,
    role: &'static str,
    permissions: Vec<&'static str>,
    node_kind: NodeKind,
    instance_name: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> AppResult<Response> {
    web_session::validate_origin(&headers, &state.config)?;
    let username = request.username.trim().to_owned();
    auth::check_login_allowed(&username)?;
    let found = repository::find_account_by_username(&state.pool, &username).await?;
    let account = match found {
        Some((account, hash)) if !account.disabled => password::verify(request.password, hash)
            .await
            .then_some(account),
        _ => {
            password::verify_decoy(request.password).await;
            None
        }
    };
    let Some(account) = account else {
        auth::record_login_failure(&username);
        return Err(AppError::Unauthorized);
    };
    auth::record_login_success(&username);
    open_session(&state, &account).await
}

/// Ouvre une session web pour ce compte et renvoie ses cookies.
pub async fn open_session(state: &AppState, account: &repository::Account) -> AppResult<Response> {
    let session = web_session::create(
        &state.pool,
        state.config.web_session_ttl_seconds,
        &account.id,
    )
    .await?;
    session_response(session, &account.username, &state.config)
}

pub async fn current(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if headers.contains_key(header::ORIGIN) {
        web_session::validate_origin(&headers, &state.config)?;
    }
    let stored = web_session::from_headers(&state.pool, &headers).await?;
    let csrf_token = web_session::validate_csrf_cookie(&headers, &stored)?.to_owned();
    let (account, _) = repository::get_account(&state.pool, &stored.account_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(no_store(
        Json(SessionResponse {
            csrf_token,
            expires_at: stored.expires_at.to_rfc3339(),
            username: account.username,
        })
        .into_response(),
    ))
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    web_session::validate_origin(&headers, &state.config)?;
    let session = web_session::from_headers(&state.pool, &headers).await?;
    web_session::validate_csrf(&headers, &session)?;
    web_session::revoke(&state.pool, &session).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_cookies(&mut response, web_session::clear_cookies(&state.config))?;
    Ok(no_store(response))
}

pub async fn me(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
) -> AppResult<Json<MeResponse>> {
    let instance_name = repository::get_setting(&state.pool, repository::INSTANCE_NAME_KEY).await?;
    Ok(Json(MeResponse {
        kind: match principal.kind {
            PrincipalKind::Account => "account",
            PrincipalKind::ApiKey => "api_key",
        },
        name: principal.name.clone(),
        role: principal.role.as_str(),
        permissions: principal
            .role
            .permissions()
            .into_iter()
            .map(flux_shared::Permission::as_str)
            .collect(),
        node_kind: NodeKind::Central,
        instance_name,
    }))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

/// Change le mot de passe du compte connecté ; ses autres sessions sont fermées.
pub async fn change_password(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Json(request): Json<ChangePasswordRequest>,
) -> AppResult<StatusCode> {
    if principal.kind != PrincipalKind::Account {
        return Err(AppError::Forbidden);
    }
    auth::check_login_allowed(&principal.name)?;
    password::validate_password(&request.new_password)?;
    let (_, stored_hash) = repository::get_account(&state.pool, &principal.id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !password::verify(request.current_password, stored_hash).await {
        auth::record_login_failure(&principal.name);
        return Err(AppError::Unauthorized);
    }
    auth::record_login_success(&principal.name);
    let new_hash = password::hash(request.new_password).await?;
    repository::update_password(
        &state.pool,
        &principal.id,
        &new_hash,
        principal.session_hash.as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn session_response(
    session: WebSession,
    username: &str,
    config: &crate::config::Config,
) -> AppResult<Response> {
    let mut response = Json(SessionResponse {
        csrf_token: session.csrf_token.clone(),
        expires_at: session.expires_at.to_rfc3339(),
        username: username.to_owned(),
    })
    .into_response();
    set_cookies(
        &mut response,
        [
            web_session::session_cookie(&session, config),
            web_session::csrf_cookie(&session, config),
        ],
    )?;
    Ok(no_store(response))
}

fn set_cookies(response: &mut Response, cookies: [String; 2]) -> AppResult<()> {
    for cookie in cookies {
        let value = HeaderValue::from_str(&cookie).map_err(|_| AppError::Internal)?;
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    Ok(())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
