//! Routes du central réservées à la clé d'un worker, limitées à ce worker (ADR 0011, phase 3).

use super::{AppState, session};
use crate::{
    auth::{self, Principal},
    domain::{Job, Worker},
    error::AppResult,
    plugins::{self, CatalogEntry},
    repository,
    worker_view::{
        AccountSnapshot, JobCounts, VerifiedAccount, VerifyLoginRequest, WORKER_JOBS_LIMIT,
        WorkerOverview,
    },
};
use axum::{Json, extract::State, http::HeaderMap};

/// Le worker qui appelle : sa clé donne son nom, et son nom retrouve sa ligne de worker.
async fn calling_worker(
    state: &AppState,
    headers: &HeaderMap,
) -> AppResult<(Principal, Option<Worker>)> {
    let principal = auth::authorize_worker(&state.pool, headers).await?;
    let worker = repository::find_worker_by_name(&state.pool, &principal.name).await?;
    Ok((principal, worker))
}

pub async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<WorkerOverview>> {
    let (principal, worker) = calling_worker(&state, &headers).await?;
    let mut jobs = JobCounts::default();
    if let Some(worker) = &worker {
        for (status, count) in repository::count_jobs_for_worker(&state.pool, worker.id).await? {
            jobs.add(status, count);
        }
    }
    Ok(Json(WorkerOverview {
        name: principal.name,
        worker_id: worker.as_ref().map(|worker| worker.id),
        last_seen: worker.map(|worker| worker.last_seen),
        jobs,
    }))
}

/// Tâches traitées par le worker appelant, et par lui seul.
pub async fn jobs(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Vec<Job>>> {
    let (_, worker) = calling_worker(&state, &headers).await?;
    let jobs = match worker {
        Some(worker) => {
            repository::list_jobs_for_worker(&state.pool, worker.id, WORKER_JOBS_LIMIT).await?
        }
        None => Vec::new(),
    };
    Ok(Json(jobs))
}

pub async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<CatalogEntry>>> {
    calling_worker(&state, &headers).await?;
    let roots = plugins::catalog_roots(&state.pool, &state.config).await;
    Ok(Json(plugins::list_catalog(&roots)))
}

/// Connexion déléguée : le central vérifie les identifiants saisis sur le worker, avec la même
/// limitation d'essais qu'une connexion directe, ce qui rend révocation et changement de mot de passe
/// immédiats.
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifyLoginRequest>,
) -> AppResult<Json<VerifiedAccount>> {
    calling_worker(&state, &headers).await?;
    let account =
        session::verify_credentials(&state.pool, &request.username, request.password).await?;
    Ok(Json(VerifiedAccount {
        id: account.id,
        username: account.username,
        role: account.role,
    }))
}

/// Copie des comptes pour la connexion de secours du worker (central injoignable).
pub async fn accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<AccountSnapshot>> {
    calling_worker(&state, &headers).await?;
    Ok(Json(AccountSnapshot {
        accounts: repository::list_account_copies(&state.pool).await?,
    }))
}
