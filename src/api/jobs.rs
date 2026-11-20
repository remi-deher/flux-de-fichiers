use super::{
    AppState, ManageJobsAuth, ManageWorkersAuth, ViewJobsAuth, ViewLibrariesAuth, ViewOverviewAuth,
    ViewWorkersAuth,
};
use crate::{
    domain::*,
    error::{AppError, AppResult},
    repository, storage,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};
use uuid::Uuid;

/// Intervalle des messages de maintien du flux d'événements, pour traverser les proxys.
const SSE_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

pub async fn files(
    State(s): State<AppState>,
    _auth: ViewLibrariesAuth,
) -> AppResult<Json<Vec<MediaFile>>> {
    Ok(Json(
        storage::list_inputs(&s.config.input_dir, s.config.max_file_bytes).await?,
    ))
}

pub async fn create(
    State(s): State<AppState>,
    _auth: ManageJobsAuth,
    Json(r): Json<NewJob>,
) -> AppResult<(StatusCode, Json<Job>)> {
    let path = storage::input_path(&s.config.input_dir, &r.source_name).await?;
    let (hash, bytes) = storage::fingerprint(&path, s.config.max_file_bytes).await?;
    if let Some(worker) = r.target_worker_id
        && !repository::worker_exists(&s.pool, worker).await?
    {
        return Err(AppError::Invalid);
    }
    let job = repository::insert_job_with_priority(
        &s.pool,
        &r.source_name,
        r.target_worker_id,
        &hash,
        bytes as i64,
        r.library_id,
        r.flow_id.as_deref(),
        r.priority,
    )
    .await?;
    s.notify_job_event(JobEventType::JobCreated, job.id, JobStatus::Queued, None);
    Ok((StatusCode::CREATED, Json(job)))
}

#[derive(Deserialize)]
pub struct Page {
    limit: Option<i64>,
    before: Option<Uuid>,
    #[serde(flatten)]
    filter: forgearr_shared::jobs::JobListFilter,
}
pub async fn list(
    State(s): State<AppState>,
    Query(p): Query<Page>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<Job>>> {
    let limit = p.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(AppError::Invalid);
    }
    Ok(Json(
        repository::filtered_jobs(&s.pool, p.before, limit, &p.filter).await?,
    ))
}

struct EventsCursor {
    replay: std::collections::VecDeque<(u64, JobEvent)>,
    rx: tokio::sync::broadcast::Receiver<(u64, JobEvent)>,
    last_seq: u64,
}

fn format_sse_event(seq: u64, event: &JobEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_default();
    Event::default()
        .id(seq.to_string())
        .event(event.event_type.as_str())
        .data(data)
}

pub async fn events(
    State(s): State<AppState>,
    h: HeaderMap,
    _auth: ViewJobsAuth,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let last_event_id = h
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.trim().parse::<u64>().ok());
    let (replay, rx) = s.event_history.subscribe_from(last_event_id).await;
    let cursor = EventsCursor {
        replay: replay.into(),
        rx,
        last_seq: last_event_id.unwrap_or(0),
    };
    let stream = futures_util::stream::unfold(cursor, |mut cursor| async move {
        if let Some((seq, event)) = cursor.replay.pop_front() {
            cursor.last_seq = cursor.last_seq.max(seq);
            return Some((Ok(format_sse_event(seq, &event)), cursor));
        }
        loop {
            match cursor.rx.recv().await {
                Ok((seq, event)) => {
                    if seq <= cursor.last_seq {
                        continue;
                    }
                    cursor.last_seq = seq;
                    return Some((Ok(format_sse_event(seq, &event)), cursor));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEP_ALIVE_INTERVAL)))
}

/// Journal ordonné des étapes exécutées pour une tâche.
pub async fn steps(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<JobStepLog>>> {
    Ok(Json(repository::list_job_steps(&s.pool, id).await?))
}

/// Remontée par un worker de la trace d'exécution des nœuds d'une tâche.
pub async fn report_steps(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<StepsReport>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let lease = LeaseRequest {
        worker_id: r.worker_id,
        lease_token: r.lease_token,
        lease_epoch: r.lease_epoch,
        telemetry: None,
    };
    repository::save_job_steps_for_lease(&s.pool, id, &lease, &r.steps).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_job(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Job>> {
    Ok(Json(repository::get_job(&s.pool, id).await?))
}

/// Supprime ou annule une tâche qui n'est pas en cours d'exécution.
pub async fn delete_job(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageJobsAuth,
) -> AppResult<StatusCode> {
    repository::delete_job(&s.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
/// Relance manuelle d'une tâche échouée après perte de bail (aucune relance automatique).
pub async fn retry(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageJobsAuth,
) -> AppResult<Json<Job>> {
    let job = repository::retry_job(&s.pool, id).await?;
    s.notify_job_event(JobEventType::JobRetried, job.id, JobStatus::Queued, None);
    Ok(Json(job))
}

/// Annule activement une tâche en attente ou en cours d'exécution.
pub async fn cancel(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageJobsAuth,
) -> AppResult<Json<Job>> {
    let job = repository::cancel_job(&s.pool, id).await?;
    s.notify_job_event(JobEventType::JobCanceled, job.id, JobStatus::Canceled, None);
    Ok(Json(job))
}

/// Met à jour la priorité d'une tâche en attente.
pub async fn update_priority(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageJobsAuth,
    Json(r): Json<UpdateJobPriorityRequest>,
) -> AppResult<Json<Job>> {
    Ok(Json(
        repository::update_job_priority(&s.pool, id, r.priority).await?,
    ))
}

/// Applique une action (`retry`, `cancel`, `delete`) à un lot de tâches.
pub async fn batch(
    State(s): State<AppState>,
    _auth: ManageJobsAuth,
    Json(r): Json<JobBatchRequest>,
) -> AppResult<Json<JobBatchResult>> {
    let (result, events) = repository::batch_jobs(&s.pool, &r.ids, r.action).await?;
    for (event_type, job_id, status) in events {
        s.notify_job_event(event_type, job_id, status, None);
    }
    Ok(Json(result))
}

/// Statistiques globales et économies d'espace disque agrégées par bibliothèque.
pub async fn stats(
    State(s): State<AppState>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<JobStorageStats>> {
    Ok(Json(repository::job_storage_stats(&s.pool).await?))
}

pub async fn workers(
    State(s): State<AppState>,
    _auth: ViewWorkersAuth,
) -> AppResult<Json<Vec<Worker>>> {
    Ok(Json(repository::list_workers(&s.pool).await?))
}

/// Met à jour la concurrence maximale et la fenêtre horaire d'un worker.
pub async fn update_worker_settings(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageWorkersAuth,
    Json(r): Json<UpdateWorkerSettingsRequest>,
) -> AppResult<Json<Worker>> {
    Ok(Json(
        repository::update_worker_settings(&s.pool, id, r.max_concurrency, r.schedule.as_ref())
            .await?,
    ))
}

pub async fn delete_worker(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageWorkersAuth,
) -> AppResult<StatusCode> {
    repository::delete_worker(&s.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn heartbeat(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<LeaseRequest>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    repository::heartbeat(&s.pool, id, &r).await?;
    s.notify_job_event(JobEventType::JobHeartbeat, id, JobStatus::Running, None);
    Ok(StatusCode::NO_CONTENT)
}

/// Progression éphémère d'un worker distant → bus SSE `job_progress` (même contrat que le local).
pub async fn progress(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<ProgressReport>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    if r.progress > 100 {
        return Err(AppError::Invalid);
    }
    repository::active(
        &s.pool,
        id,
        &LeaseRequest {
            worker_id: r.worker_id,
            lease_token: r.lease_token,
            lease_epoch: r.lease_epoch,
            telemetry: None,
        },
    )
    .await?;
    s.notify_job_progress(id, r.progress, r.message);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn failure(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<Failure>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let error_code: JobErrorCode = r.error_code.parse().map_err(|_| AppError::Invalid)?;
    if !error_code.is_worker_reportable() {
        return Err(AppError::Invalid);
    }
    repository::fail(
        &s.pool,
        id,
        &LeaseRequest {
            worker_id: r.worker_id,
            lease_token: r.lease_token,
            lease_epoch: r.lease_epoch,
            telemetry: None,
        },
        error_code,
    )
    .await?;
    s.notify_job_event(
        JobEventType::JobFailed,
        id,
        JobStatus::Failed,
        Some(error_code),
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct RelayInfoQuery {
    ids: String,
}

/// Marques « relayée » des tâches listées, pour l'interface (200 identifiants au plus).
pub async fn relay_infos(
    State(s): State<AppState>,
    Query(query): Query<RelayInfoQuery>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<forgearr_shared::relay::JobRelayInfo>>> {
    let ids = query
        .ids
        .split(',')
        .take(200)
        .map(|id| id.parse::<Uuid>().map_err(|_| AppError::Invalid))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Json(repository::list_relays(&s.pool, &ids).await?))
}

/// Relaie la tâche d'un worker vers la file d'un autre worker (ADR 0018).
pub async fn relay(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(report): Json<forgearr_shared::relay::RelayReport>,
) -> AppResult<Json<forgearr_shared::relay::RelayResponse>> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let lease = LeaseRequest {
        worker_id: report.worker_id,
        lease_token: report.lease_token,
        lease_epoch: report.lease_epoch,
        telemetry: None,
    };
    let response = match repository::relay_job(&s.pool, id, &lease, &report.request, report.warning).await? {
        repository::RelayOutcome::Relayed { target_worker_id } => {
            s.notify_job_event(JobEventType::JobRelayed, id, JobStatus::Queued, None);
            forgearr_shared::relay::RelayResponse {
                relayed: true,
                target_worker_id,
            }
        }
        repository::RelayOutcome::Failed => {
            s.notify_job_event(
                JobEventType::JobFailed,
                id,
                JobStatus::Failed,
                Some(JobErrorCode::NoCapableWorker),
            );
            forgearr_shared::relay::RelayResponse {
                relayed: false,
                target_worker_id: None,
            }
        }
    };
    Ok(Json(response))
}

pub async fn system_config(
    State(s): State<AppState>,
    _auth: ViewOverviewAuth,
) -> AppResult<Json<SystemConfig>> {
    Ok(Json(SystemConfig {
        max_file_bytes: s.config.max_file_bytes,
        local_worker: s.config.local_worker,
        version: crate::version::app_version().to_owned(),
    }))
}

pub async fn placement(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<forgearr_shared::libraries::JobPlacement>> {
    use forgearr_shared::{flows::FlowGraph, libraries::JobPlacement};
    let job = repository::get_job(&s.pool, id).await?;
    if job.status != JobStatus::Queued {
        return Ok(Json(JobPlacement::default()));
    }
    let flow = match (job.flow_id.as_deref(), job.flow_version) {
        (Some(id), Some(version)) => {
            Some(repository::get_flow_version(&s.pool, id, version).await?)
        }
        _ => None,
    };
    let hardware_requested = flow
        .as_ref()
        .and_then(|flow| serde_json::from_str::<FlowGraph>(&flow.definition).ok())
        .is_some_and(|graph| {
            graph.nodes.iter().any(|node| {
                matches!(
                    node.config
                        .get("encoder")
                        .and_then(serde_json::Value::as_str),
                    Some("nvidia" | "intel_qsv" | "vaapi" | "amd_amf")
                )
            })
        });
    let roots = crate::plugins::catalog_roots(&s.pool, &s.config).await;
    let registry = s.catalog_cache.operations(&roots).await;
    let catalog_tools_unavailable = registry
        .required_tool_refs_for_flow(flow.as_ref())
        .into_iter()
        .filter(|tool| !crate::plugins::is_tool_installable(&roots, &tool.id))
        .map(|tool| tool.id)
        .collect();
    let workers = repository::library_history::placement(&s.pool, &job, hardware_requested).await?;
    Ok(Json(JobPlacement {
        workers,
        catalog_tools_unavailable,
    }))
}

#[derive(Deserialize)]
pub struct FailureDetailsQuery {
    ids: String,
}

pub async fn failure_details(
    State(s): State<AppState>,
    Query(query): Query<FailureDetailsQuery>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<forgearr_shared::jobs::JobFailureDetail>>> {
    let mut details = Vec::new();
    for id in query.ids.split(',').take(200) {
        let id = id.parse::<Uuid>().map_err(|_| AppError::Invalid)?;
        if let Some((step_index, block_kind, message)) = sqlx::query_as::<_, (i32, String, Option<String>)>(
            "SELECT step_index,block_kind,message FROM job_steps WHERE job_id=? AND status='failed' ORDER BY step_index DESC LIMIT 1"
        ).bind(id).fetch_optional(&s.pool).await? {
            details.push(forgearr_shared::jobs::JobFailureDetail {job_id:id, step_index,block_kind,message});
        }
    }
    Ok(Json(details))
}

pub async fn replay_failures(
    State(s): State<AppState>,
    _auth: ManageJobsAuth,
    Json(request): Json<forgearr_shared::jobs::FailureReplay>,
) -> AppResult<Json<JobBatchResult>> {
    const MAX_BATCH: usize = 100;
    if request.ids.is_empty() || request.ids.len() > MAX_BATCH {
        return Err(AppError::Invalid);
    }
    let mut result = JobBatchResult {
        updated: 0,
        skipped: 0,
    };
    for id in request
        .ids
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
    {
        let job = match repository::get_job(&s.pool, id).await {
            Ok(job) => job,
            Err(AppError::NotFound) => {
                result.skipped += 1;
                continue;
            }
            Err(error) => return Err(error),
        };
        let Some(library) = job.library_id.filter(|_| job.status == JobStatus::Failed) else {
            result.skipped += 1;
            continue;
        };
        match repository::library_history::reprocess(&s.pool, library, id, request.version).await {
            Ok(next) => {
                s.notify_job_event(JobEventType::JobCreated, next.id, JobStatus::Queued, None);
                result.updated += 1;
            }
            Err(AppError::Conflict | AppError::NotFound | AppError::Invalid) => result.skipped += 1,
            Err(error) => return Err(error),
        }
    }
    Ok(Json(result))
}

pub async fn retry_notifications(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageJobsAuth,
) -> AppResult<Json<usize>> {
    let job = repository::get_job(&s.pool, id).await?;
    if job.status != JobStatus::Succeeded {
        return Err(AppError::Conflict);
    }
    let name = job
        .output_name
        .as_deref()
        .filter(|name| storage::valid_name(name))
        .ok_or(AppError::NotFound)?;
    let output = s.config.data_dir.join("output").join(name);
    if !tokio::fs::metadata(&output)
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return Err(AppError::NotFound);
    }
    Ok(Json(
        crate::flows::publication::retry_notifications(&s, &job, &output).await?,
    ))
}
