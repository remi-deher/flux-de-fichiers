//! Mot de passe oublié par courriel : demande de lien, puis nouveau mot de passe (ADR 0015).

use super::AppState;
use crate::{
    error::{AppError, AppResult},
    mail::SmtpSender,
    password_reset, web_session,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
pub use flux_shared::accounts::{ForgotPasswordRequest, ResetPasswordRequest};

/// Demande un lien. La réponse est `202` que l'adresse corresponde à un compte ou non, et l'envoi se
/// fait en arrière-plan pour que le délai ne le révèle pas non plus.
pub async fn forgot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ForgotPasswordRequest>,
) -> AppResult<StatusCode> {
    web_session::validate_origin(&headers, &state.config)?;
    let (Some(smtp), Some(origin)) = (state.config.smtp.clone(), state.config.web_origin.clone())
    else {
        return Err(AppError::NotFound);
    };
    let email = password_reset::admit_request(&request.email)?;
    tokio::spawn(async move {
        if let Err(error) =
            password_reset::issue_and_send(&state.pool, &SmtpSender(&smtp), &origin, &email).await
        {
            tracing::error!(error = ?error, "password reset request failed");
        }
    });
    Ok(StatusCode::ACCEPTED)
}

/// Change le mot de passe avec le jeton reçu ; un jeton inconnu, expiré ou déjà utilisé donne `400`.
pub async fn reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ResetPasswordRequest>,
) -> AppResult<StatusCode> {
    web_session::validate_origin(&headers, &state.config)?;
    password_reset::confirm_reset(&state.pool, &request.token, &request.new_password).await?;
    Ok(StatusCode::NO_CONTENT)
}
