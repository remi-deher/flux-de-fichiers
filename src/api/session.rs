use super::AppState;
use crate::{
    auth,
    error::{AppError, AppResult},
    web_session::{self, WebSession},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Default)]
pub struct LoginRequest {
    #[serde(default)]
    admin_key: Option<String>,
}

#[derive(Serialize)]
pub struct SessionResponse {
    csrf_token: String,
    expires_at: String,
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> AppResult<Response> {
    web_session::validate_origin(&headers, &state.config)?;
    if let Some(ref key) = request.admin_key
        && !key.trim().is_empty()
    {
        auth::verify_key(key.trim(), &state.config.admin_key)?;
    }
    let session = web_session::create(&state.pool, state.config.web_session_ttl_seconds).await?;
    session_response(session, &state.config)
}

pub async fn current(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if headers.contains_key(header::ORIGIN) {
        web_session::validate_origin(&headers, &state.config)?;
    }
    let stored = web_session::from_headers(&state.pool, &headers).await?;
    let csrf_token = web_session::validate_csrf_cookie(&headers, &stored)?.to_owned();
    Ok(no_store(
        Json(SessionResponse {
            csrf_token,
            expires_at: stored.expires_at.to_rfc3339(),
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

fn session_response(session: WebSession, config: &crate::config::Config) -> AppResult<Response> {
    let mut response = Json(SessionResponse {
        csrf_token: session.csrf_token.clone(),
        expires_at: session.expires_at.to_rfc3339(),
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
