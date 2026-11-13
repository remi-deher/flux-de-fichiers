//! Extracteurs d'authentification et de sécurité pour Axum.

use super::AppState;
use crate::{
    auth::{self, Principal},
    error::AppError,
};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use flux_shared::Permission;

/// Appelant authentifié, quel que soit son rôle (compte connecté ou clé d'API).
#[derive(Debug, Clone)]
pub struct AnyAuth(pub Principal);

impl<S> FromRequestParts<S> for AnyAuth
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        auth::authenticate(
            &app_state.pool,
            &app_state.config,
            &parts.headers,
            &parts.method,
        )
        .await
        .map(AnyAuth)
    }
}

/// Appelant ayant le rôle administrateur : compte connecté (session, CSRF et origine pour les
/// requêtes qui modifient) ou clé d'API de rôle administrateur (`Authorization: Bearer`).
#[derive(Debug, Clone)]
pub struct AdminAuth(pub Principal);

impl<S> FromRequestParts<S> for AdminAuth
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AnyAuth(principal) = AnyAuth::from_request_parts(parts, state).await?;
        auth::require(&principal, Permission::ManageSettings)?;
        Ok(AdminAuth(principal))
    }
}
