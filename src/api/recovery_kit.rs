//! Kit de récupération de la clé de chiffrement : export protégé par phrase de passe et
//! restauration après remise en place d'une base (ADR 0011).

use super::{AdminAuth, AppState, system::current_status};
use crate::{
    error::{AppError, AppResult},
    secret_rotation,
    secrets::{self, recovery_kit},
};
use axum::{Json, extract::State};
use flux_shared::system::{
    ExportRecoveryKitRequest, RecoveryKit, RestoreRecoveryKitRequest, SystemStatus,
};

/// Argon2id est volontairement coûteux : il tourne hors du runtime asynchrone.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| AppError::Internal)?
}

/// Exporte le trousseau chiffré par la phrase de passe et note la clé couverte par ce kit.
pub async fn export(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<ExportRecoveryKitRequest>,
) -> AppResult<Json<RecoveryKit>> {
    let config_dir = state.config.config_dir.clone();
    let kit = blocking(move || {
        let kit = recovery_kit::export(
            &config_dir,
            secrets::external_key_file().as_deref(),
            &request.passphrase,
        )?;
        recovery_kit::mark_exported(&config_dir, &kit.key_id)?;
        Ok(kit)
    })
    .await?;
    tracing::info!(key_id = %kit.key_id, "kit de récupération exporté");
    Ok(Json(kit))
}

/// Ouvre le kit, reprend ses clés dans le trousseau puis rechiffre tout sous la clé active.
/// 400 si la phrase de passe est fausse ou le kit altéré ; 409 si une rotation est en cours.
pub async fn restore(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<RestoreRecoveryKitRequest>,
) -> AppResult<Json<SystemStatus>> {
    let keys = blocking(move || recovery_kit::open(&request.kit, &request.passphrase)).await?;
    let report = secret_rotation::restore_keys(
        &state.pool,
        &state.config.config_dir,
        secrets::external_key_file().as_deref(),
        &keys,
    )
    .await?;
    tracing::info!(
        plugin_values = report.plugin_values,
        flow_values = report.flow_values,
        "kit de récupération restauré, secrets rechiffrés sous la clé active"
    );
    current_status(&state).await
}
