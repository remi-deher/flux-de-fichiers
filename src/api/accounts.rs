//! Gestion des comptes : liste, création, changement de rôle, désactivation (ADR 0011).
//!
//! Un compte ne se supprime pas : il se désactive, ce qui ferme ses sessions et garde son historique.
//! La centrale garde toujours au moins un administrateur actif.

use super::{AppState, ManageAccountsAuth};
use crate::{
    error::{AppError, AppResult},
    password,
    repository::{self, Account},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use flux_shared::Role;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateAccountRequest {
    username: String,
    password: String,
    role: String,
}

#[derive(Deserialize)]
pub struct UpdateAccountRequest {
    role: Option<String>,
    disabled: Option<bool>,
}

/// Rôle d'un compte humain : un rôle inconnu ou réservé aux machines est refusé.
fn account_role(raw: &str) -> AppResult<Role> {
    Role::parse(raw)
        .filter(|role| role.is_account_role())
        .ok_or(AppError::Invalid)
}

pub async fn list(
    State(state): State<AppState>,
    _auth: ManageAccountsAuth,
) -> AppResult<Json<Vec<Account>>> {
    Ok(Json(repository::list_accounts(&state.pool).await?))
}

pub async fn create(
    State(state): State<AppState>,
    _auth: ManageAccountsAuth,
    Json(request): Json<CreateAccountRequest>,
) -> AppResult<(StatusCode, Json<Account>)> {
    let role = account_role(&request.role)?;
    let username = password::validate_username(&request.username)?;
    password::validate_password(&request.password)?;
    let hash = password::hash(request.password).await?;
    let account = repository::create_account(&state.pool, &username, &hash, role).await?;
    Ok((StatusCode::CREATED, Json(account)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _auth: ManageAccountsAuth,
    Json(request): Json<UpdateAccountRequest>,
) -> AppResult<Json<Account>> {
    let role = request.role.as_deref().map(account_role).transpose()?;
    if role.is_none() && request.disabled.is_none() {
        return Err(AppError::Invalid);
    }
    Ok(Json(
        repository::update_account(&state.pool, &id, role, request.disabled).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_human_roles_are_accepted_for_accounts() {
        assert_eq!(account_role("admin").ok(), Some(Role::Admin));
        assert_eq!(account_role("viewer").ok(), Some(Role::Viewer));
        assert!(account_role("worker").is_err());
        assert!(account_role("root").is_err());
    }
}
