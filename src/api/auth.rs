//! Extracteurs d'authentification et de sécurité pour Axum.

use super::AppState;
use crate::error::AppError;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

/// Extracteur validant que la requête provient d'un administrateur authentifié.
///
/// Vérifie soit la présence du token Bearer administrateur (`Authorization: Bearer <key>`),
/// soit un cookie de session valide avec vérification CSRF et Origin pour les requêtes mutantes.
#[derive(Debug, Clone, Copy)]
pub struct AdminAuth;

impl<S> FromRequestParts<S> for AdminAuth
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        crate::web_session::authorize_admin(
            &app_state.pool,
            &app_state.config,
            &parts.headers,
            &parts.method,
        )
        .await?;
        Ok(AdminAuth)
    }
}
