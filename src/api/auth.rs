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

/// Déclare un extracteur qui exige un droit : compte connecté (session, CSRF et origine pour les
/// requêtes qui modifient) ou clé d'API (`Authorization: Bearer`) portant ce droit.
macro_rules! permission_auth {
    ($(#[$doc:meta])* $name:ident, $permission:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $name(pub Principal);

        impl<S> FromRequestParts<S> for $name
        where
            AppState: FromRef<S>,
            S: Send + Sync,
        {
            type Rejection = AppError;

            async fn from_request_parts(
                parts: &mut Parts,
                state: &S,
            ) -> Result<Self, Self::Rejection> {
                let AnyAuth(principal) = AnyAuth::from_request_parts(parts, state).await?;
                auth::require(&principal, $permission)?;
                Ok($name(principal))
            }
        }
    };
}

permission_auth!(
    /// Administrateur : pilote les réglages, les plugins et tout ce qui modifie.
    AdminAuth,
    Permission::ManageSettings
);
permission_auth!(
    /// Peut gérer les comptes.
    ManageAccountsAuth,
    Permission::ManageAccounts
);
permission_auth!(
    /// Peut consulter la vue d'ensemble et la configuration système.
    ViewOverviewAuth,
    Permission::ViewOverview
);
permission_auth!(
    /// Peut consulter les tâches.
    ViewJobsAuth,
    Permission::ViewJobs
);
permission_auth!(
    /// Peut consulter les nœuds.
    ViewNodesAuth,
    Permission::ViewNodes
);
permission_auth!(
    /// Peut consulter les bibliothèques et les fichiers sources.
    ViewLibrariesAuth,
    Permission::ViewLibraries
);
permission_auth!(
    /// Peut consulter les flux.
    ViewFlowsAuth,
    Permission::ViewFlows
);
permission_auth!(
    /// Peut consulter les plugins, leur catalogue et leurs dépôts.
    ViewPluginsAuth,
    Permission::ViewPlugins
);
