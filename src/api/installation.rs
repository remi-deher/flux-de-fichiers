//! Enrôlement des workers : demande à approuver (ADR 0016) et secours par identifiants (ADR 0011).

use super::{AppState, ManageWorkersAuth, ViewWorkersAuth, client_remote_addr, session};
use crate::{
    config, enrollment,
    error::{AppError, AppResult},
    instance, repository, web_session, worker_settings,
};
use axum::{
    Extension, Json,
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
pub use flux_shared::system::{
    SetupWorkerApprovalRequest, SetupWorkerRequest, SetupWorkerRequestStatus,
    WorkerSetupRequestState,
};
use flux_shared::workers::{
    EnrollmentRequestStatus, EnrollmentWindowStatus, WorkerEnrollmentCreated, WorkerEnrollmentList,
    WorkerEnrollmentRequestInput, WorkerEnrollmentSummary,
};
use flux_shared::{InstanceKind, Role};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, time::Duration};
use tokio::task::JoinHandle;
use uuid::Uuid;

const WORKER_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// État en mémoire d'une demande d'enrôlement en cours sur une installation vierge.
pub struct PendingWorkerSetup {
    request_id: Uuid,
    status: SetupWorkerRequestStatus,
    task: Option<JoinHandle<()>>,
}

impl Drop for PendingWorkerSetup {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// État public de l'installation, identique pour le central et le worker (sondes, supervision).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    instance_kind: InstanceKind,
    version: &'static str,
    installed: bool,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<InstanceStatus>> {
    Ok(Json(InstanceStatus {
        instance_kind: InstanceKind::Central,
        version: crate::version::app_version(),
        installed: repository::account_count(&state.pool).await? > 0,
    }))
}

#[derive(Deserialize)]
pub struct EnrollmentWindowCommand {
    #[serde(default = "default_window_open")]
    pub open: bool,
}

const fn default_window_open() -> bool {
    true
}

/// Ouvre (pour 15 minutes) ou ferme la fenêtre d'acceptation des demandes de workers.
pub async fn set_enrollment_window(
    State(state): State<AppState>,
    _auth: ManageWorkersAuth,
    body: Bytes,
) -> AppResult<Json<EnrollmentWindowStatus>> {
    let open = if body.is_empty() {
        true
    } else {
        serde_json::from_slice::<EnrollmentWindowCommand>(&body)
            .map_err(|_| AppError::Invalid)?
            .open
    };
    Ok(Json(
        repository::set_enrollment_window(&state.pool, open).await?,
    ))
}

/// Liste l'état de la fenêtre et les demandes d'enrôlement en attente (écran Workers).
pub async fn list_enrollment_requests(
    State(state): State<AppState>,
    _auth: ViewWorkersAuth,
) -> AppResult<Json<WorkerEnrollmentList>> {
    Ok(Json(
        repository::list_enrollment_requests(&state.pool).await?,
    ))
}

/// Reçoit une demande d'enrôlement d'un worker (ADR 0016). Refusée si la fenêtre est fermée.
pub async fn submit_enrollment_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(request): Json<WorkerEnrollmentRequestInput>,
) -> AppResult<(StatusCode, Json<WorkerEnrollmentCreated>)> {
    if !worker_settings::is_valid_worker_name(&request.worker_name)
        || !enrollment::is_valid_sha256_hex(&request.secret_hash)
    {
        return Err(AppError::Invalid);
    }
    let source_addr = client_remote_addr(&headers, connect_info);
    enrollment::check_request_rate_limit(source_addr.as_deref())?;
    let version = request
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= 64);
    let created = repository::create_enrollment_request(
        &state.pool,
        &request.worker_name,
        version,
        source_addr.as_deref(),
        &request.secret_hash,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Interroge l'état d'une demande d'enrôlement ; remet la clé une seule fois à l'approbation.
pub async fn poll_enrollment_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let secret = headers
        .get(enrollment::ENROLLMENT_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Unauthorized)?;
    let reply = repository::poll_enrollment_request(&state.pool, id, secret).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(reply)).into_response())
}

/// Approuve une demande d'enrôlement en attente.
pub async fn approve_enrollment_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageWorkersAuth,
) -> AppResult<Json<WorkerEnrollmentSummary>> {
    Ok(Json(
        repository::decide_enrollment_request(&state.pool, id, true).await?,
    ))
}

/// Refuse une demande d'enrôlement en attente.
pub async fn reject_enrollment_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageWorkersAuth,
) -> AppResult<Json<WorkerEnrollmentSummary>> {
    Ok(Json(
        repository::decide_enrollment_request(&state.pool, id, false).await?,
    ))
}

/// Crée la clé d'API d'un nouveau worker. Réservé aux identifiants d'un administrateur ; la clé
/// n'est renvoyée qu'ici. Pas de cookie : l'appel vient du worker, jamais d'un navigateur.
pub async fn enroll(
    State(state): State<AppState>,
    Json(request): Json<enrollment::EnrollRequest>,
) -> AppResult<Response> {
    if !worker_settings::is_valid_worker_name(&request.worker_name) {
        return Err(AppError::Invalid);
    }
    let account =
        session::verify_credentials(&state.pool, &request.username, request.password).await?;
    if account.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let issued = crate::api_keys::issue(&state.pool, &request.worker_name, Role::Worker).await?;
    let body = Json(enrollment::EnrolledWorker { key: issued.raw });
    Ok(([(header::CACHE_CONTROL, "no-store")], body).into_response())
}

async fn ensure_fresh_installation(state: &AppState) -> AppResult<()> {
    let already_worker = worker_settings::load_worker(&state.config.config_dir)
        .map_err(|error| {
            tracing::error!(error = %error, "rôle de l'installation illisible");
            AppError::Internal
        })?
        .is_some();
    if already_worker || repository::account_count(&state.pool).await? > 0 {
        return Err(AppError::AlreadyConfigured);
    }
    Ok(())
}

/// Transforme une installation vierge en worker : enrôlement auprès du central, rôle conservé dans
/// `/config`, puis redémarrage interne en worker. Une fois le central installé ou le rôle choisi : 409.
pub async fn setup_worker(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetupWorkerRequest>,
) -> AppResult<StatusCode> {
    web_session::validate_origin(&headers, &state.config)?;
    ensure_fresh_installation(&state).await?;
    if !worker_settings::is_valid_worker_name(&request.worker_name) {
        return Err(AppError::Invalid);
    }
    let central = config::central_url(request.central_url.trim()).map_err(|_| AppError::Invalid)?;
    let key = enrollment::request_key(
        &central,
        &enrollment::EnrollRequest {
            username: request.username.trim().to_owned(),
            password: request.password,
            worker_name: request.worker_name.clone(),
        },
    )
    .await?;
    worker_settings::save_worker(
        &state.config.config_dir,
        &worker_settings::WorkerSettings {
            name: request.worker_name,
            central_url: central.as_str().trim_end_matches('/').to_owned(),
        },
        &key,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "rôle worker impossible à conserver");
        AppError::Internal
    })?;
    instance::request_role_switch();
    Ok(StatusCode::ACCEPTED)
}

fn error_code_of(err: &AppError) -> &'static str {
    match err {
        AppError::Invalid => "invalid_input",
        AppError::Unauthorized => "unauthenticated",
        AppError::Forbidden => "forbidden",
        AppError::NotFound => "not_found",
        AppError::RateLimited => "too_many_attempts",
        AppError::CentralUnavailable => "central_unavailable",
        _ => "internal_error",
    }
}

fn setup_status_snapshot(
    state: WorkerSetupRequestState,
    central_url: &str,
    worker_name: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
    error_code: Option<&str>,
) -> SetupWorkerRequestStatus {
    SetupWorkerRequestStatus {
        state,
        central_url: Some(central_url.to_owned()),
        worker_name: Some(worker_name.to_owned()),
        expires_at: Some(expires_at),
        error_code: error_code.map(str::to_owned),
    }
}

async fn update_setup_request_state(
    state: &AppState,
    request_id: Uuid,
    next: SetupWorkerRequestStatus,
) {
    let mut guard = state.pending_worker_setup.lock().await;
    if let Some(pending) = guard.as_mut()
        && pending.request_id == request_id
        && matches!(pending.status.state, WorkerSetupRequestState::Pending)
    {
        pending.status = next;
    }
}

/// Lance une demande d'enrôlement par approbation depuis une installation vierge (ADR 0016).
pub async fn start_setup_worker_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetupWorkerApprovalRequest>,
) -> AppResult<(StatusCode, Json<SetupWorkerRequestStatus>)> {
    web_session::validate_origin(&headers, &state.config)?;
    ensure_fresh_installation(&state).await?;
    let trimmed_name = request.worker_name.trim();
    let worker_name = if trimmed_name.is_empty() {
        enrollment::default_worker_name()
    } else if worker_settings::is_valid_worker_name(trimmed_name) {
        trimmed_name.to_owned()
    } else {
        return Err(AppError::Invalid);
    };
    let central = config::central_url(request.central_url.trim()).map_err(|_| AppError::Invalid)?;
    let normalized_central = central.as_str().trim_end_matches('/').to_owned();
    let (secret, secret_hash) = enrollment::generate_secret_and_hash();
    let created = enrollment::submit_enrollment_request(
        &central,
        &WorkerEnrollmentRequestInput {
            worker_name: worker_name.clone(),
            version: Some(crate::version::app_version().to_owned()),
            secret_hash,
        },
    )
    .await?;

    let initial_status = setup_status_snapshot(
        WorkerSetupRequestState::Pending,
        &normalized_central,
        &worker_name,
        created.expires_at,
        None,
    );

    let bg_state = state.clone();
    let bg_central = central;
    let bg_central_str = normalized_central;
    let bg_name = worker_name;
    let req_id = created.id;
    let expires_at = created.expires_at;
    let handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(WORKER_POLL_INTERVAL).await;
            if chrono::Utc::now() >= expires_at {
                update_setup_request_state(
                    &bg_state,
                    req_id,
                    setup_status_snapshot(
                        WorkerSetupRequestState::Expired,
                        &bg_central_str,
                        &bg_name,
                        expires_at,
                        None,
                    ),
                )
                .await;
                break;
            }
            match enrollment::poll_enrollment_request(&bg_central, req_id, &secret).await {
                Ok(poll) => match poll.status {
                    EnrollmentRequestStatus::Pending => continue,
                    EnrollmentRequestStatus::Approved => {
                        let Some(key) = poll.key else {
                            update_setup_request_state(
                                &bg_state,
                                req_id,
                                setup_status_snapshot(
                                    WorkerSetupRequestState::Error,
                                    &bg_central_str,
                                    &bg_name,
                                    expires_at,
                                    Some("internal_error"),
                                ),
                            )
                            .await;
                            break;
                        };
                        if let Err(error) = worker_settings::save_worker(
                            &bg_state.config.config_dir,
                            &worker_settings::WorkerSettings {
                                name: bg_name.clone(),
                                central_url: bg_central_str.clone(),
                            },
                            &key,
                        ) {
                            tracing::error!(error = %error, "rôle worker impossible à conserver");
                            update_setup_request_state(
                                &bg_state,
                                req_id,
                                setup_status_snapshot(
                                    WorkerSetupRequestState::Error,
                                    &bg_central_str,
                                    &bg_name,
                                    expires_at,
                                    Some("internal_error"),
                                ),
                            )
                            .await;
                            break;
                        }
                        update_setup_request_state(
                            &bg_state,
                            req_id,
                            setup_status_snapshot(
                                WorkerSetupRequestState::Approved,
                                &bg_central_str,
                                &bg_name,
                                expires_at,
                                None,
                            ),
                        )
                        .await;
                        instance::request_role_switch();
                        break;
                    }
                    EnrollmentRequestStatus::Rejected => {
                        update_setup_request_state(
                            &bg_state,
                            req_id,
                            setup_status_snapshot(
                                WorkerSetupRequestState::Rejected,
                                &bg_central_str,
                                &bg_name,
                                expires_at,
                                None,
                            ),
                        )
                        .await;
                        break;
                    }
                    EnrollmentRequestStatus::Expired => {
                        update_setup_request_state(
                            &bg_state,
                            req_id,
                            setup_status_snapshot(
                                WorkerSetupRequestState::Expired,
                                &bg_central_str,
                                &bg_name,
                                expires_at,
                                None,
                            ),
                        )
                        .await;
                        break;
                    }
                },
                Err(AppError::NotFound) => {
                    update_setup_request_state(
                        &bg_state,
                        req_id,
                        setup_status_snapshot(
                            WorkerSetupRequestState::Expired,
                            &bg_central_str,
                            &bg_name,
                            expires_at,
                            None,
                        ),
                    )
                    .await;
                    break;
                }
                Err(AppError::CentralUnavailable) => continue,
                Err(err) => {
                    update_setup_request_state(
                        &bg_state,
                        req_id,
                        setup_status_snapshot(
                            WorkerSetupRequestState::Error,
                            &bg_central_str,
                            &bg_name,
                            expires_at,
                            Some(error_code_of(&err)),
                        ),
                    )
                    .await;
                    break;
                }
            }
        }
    });

    let mut guard = state.pending_worker_setup.lock().await;
    *guard = Some(PendingWorkerSetup {
        request_id: req_id,
        status: initial_status.clone(),
        task: Some(handle),
    });

    Ok((StatusCode::ACCEPTED, Json(initial_status)))
}

/// Retourne l'état courant de la demande d'enrôlement lancée depuis cette installation vierge.
pub async fn setup_worker_request_status(
    State(state): State<AppState>,
) -> AppResult<Json<SetupWorkerRequestStatus>> {
    ensure_fresh_installation(&state).await?;
    let guard = state.pending_worker_setup.lock().await;
    Ok(Json(
        guard
            .as_ref()
            .map(|pending| pending.status.clone())
            .unwrap_or_default(),
    ))
}

/// Annule l'attente locale d'approbation sur l'installation vierge.
pub async fn cancel_setup_worker_request(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    web_session::validate_origin(&headers, &state.config)?;
    ensure_fresh_installation(&state).await?;
    let mut guard = state.pending_worker_setup.lock().await;
    *guard = None;
    Ok(StatusCode::NO_CONTENT)
}
