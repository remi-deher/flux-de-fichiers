//! Installation du central au premier démarrage : nom, compte administrateur, session ouverte.

use super::{AppState, session};
use crate::{
    error::{AppError, AppResult},
    password, password_reset, repository, web_session,
};
use axum::{Json, extract::State, http::HeaderMap, response::Response};
pub use flux_shared::system::{SetupRequest, SetupStatus};
use flux_shared::{InstanceKind, Role};

const INSTANCE_NAME_MAX_LEN: usize = 80;

pub async fn status(State(state): State<AppState>) -> AppResult<Json<SetupStatus>> {
    Ok(Json(SetupStatus {
        needs_setup: repository::account_count(&state.pool).await? == 0,
        instance_name: repository::get_setting(&state.pool, repository::INSTANCE_NAME_KEY).await?,
        instance_kind: InstanceKind::Central,
        password_reset_enabled: state.config.password_reset_enabled(),
    }))
}

fn validate_instance_name(raw: &str) -> AppResult<String> {
    let name = raw.trim();
    if name.is_empty()
        || name.chars().count() > INSTANCE_NAME_MAX_LEN
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid);
    }
    Ok(name.to_owned())
}

/// Crée le compte administrateur et ouvre sa session. Utilisable une seule fois : ensuite, 409.
pub async fn setup_central(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> AppResult<Response> {
    web_session::validate_origin(&headers, &state.config)?;
    if repository::account_count(&state.pool).await? > 0 {
        return Err(AppError::AlreadyConfigured);
    }
    let name = validate_instance_name(&request.name)?;
    let username = password::validate_username(&request.username)?;
    password::validate_password(&request.password)?;
    // Le secret de chiffrement est généré maintenant, sans rien demander à l'utilisateur.
    crate::secrets::SecretBox::open(&state.config.config_dir)?;
    let email = request
        .email
        .as_deref()
        .map(password_reset::parse_optional_email)
        .transpose()?
        .flatten();
    let hash = password::hash(request.password).await?;
    let account = repository::create_first_account(
        &state.pool,
        &username,
        &hash,
        Role::Admin,
        email.as_deref(),
    )
    .await?
    .ok_or(AppError::AlreadyConfigured)?;
    repository::set_setting(&state.pool, repository::INSTANCE_NAME_KEY, &name).await?;
    session::open_session(&state, &account).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_name_rules() -> AppResult<()> {
        assert_eq!(validate_instance_name("  Mon central ")?, "Mon central");
        assert!(validate_instance_name("").is_err());
        assert!(validate_instance_name("   ").is_err());
        assert!(validate_instance_name(&"x".repeat(INSTANCE_NAME_MAX_LEN + 1)).is_err());
        assert!(validate_instance_name("tab\tici").is_err());
        Ok(())
    }
}
