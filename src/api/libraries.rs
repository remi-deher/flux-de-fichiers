//! Gestionnaires HTTP des bibliothèques déclarées, upload de médias et scan.

use super::{AppState, ManageLibrariesAuth, ViewLibrariesAuth};
use crate::{
    domain::{
        DirectoryListing, Job, JobEventType, JobStatus, Library, LibraryWrite, MediaFile,
        WorkerBrowseReport, WorkerBrowseTask,
    },
    error::{AppError, AppResult},
    filesystem_browser, repository, storage,
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const REMOTE_BROWSE_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_BROWSE_WAIT_MS: u64 = 15_000;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseQuery {
    #[serde(alias = "worker_id")]
    pub worker_id: Option<Uuid>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPendingBrowseQuery {
    #[serde(alias = "worker_id")]
    pub worker_id: Uuid,
    #[serde(default, alias = "wait_ms")]
    pub wait_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedMediaFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryUploadResult {
    pub file: UploadedMediaFile,
    pub job: Job,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryScanRequest {
    #[serde(default)]
    pub dry_run: bool,
    pub target_worker_id: Option<Uuid>,
    pub flow_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryScanResult {
    pub library_id: Uuid,
    pub dry_run: bool,
    pub scanned_files: usize,
    pub created_jobs: usize,
    pub skipped_existing: usize,
    pub created_files: Vec<String>,
}

pub async fn list(
    State(s): State<AppState>,
    _auth: ViewLibrariesAuth,
) -> AppResult<Json<Vec<Library>>> {
    Ok(Json(repository::list_libraries(&s.pool).await?))
}

pub async fn create(
    State(s): State<AppState>,
    _auth: ManageLibrariesAuth,
    Json(payload): Json<LibraryWrite>,
) -> AppResult<(StatusCode, Json<Library>)> {
    let library = repository::insert_library(&s.pool, payload).await?;
    Ok((StatusCode::CREATED, Json(library)))
}

pub async fn get(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ViewLibrariesAuth,
) -> AppResult<Json<Library>> {
    Ok(Json(repository::get_library(&s.pool, id).await?))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageLibrariesAuth,
    Json(payload): Json<LibraryWrite>,
) -> AppResult<Json<Library>> {
    Ok(Json(
        repository::update_library(&s.pool, id, payload).await?,
    ))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageLibrariesAuth,
) -> AppResult<StatusCode> {
    repository::delete_library(&s.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn write_input_stream(
    root: &std::path::Path,
    name: &str,
    max_bytes: u64,
    body: Body,
) -> AppResult<UploadedMediaFile> {
    if !storage::valid_name(name) {
        return Err(AppError::Invalid);
    }
    tokio::fs::create_dir_all(root).await?;
    let staging = root.join(format!(".upload-{}", Uuid::new_v4().simple()));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .await?;
        let mut stream = body.into_data_stream();
        let mut hash = Sha256::new();
        let mut total = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| AppError::Invalid)?;
            total += chunk.len() as u64;
            if total > max_bytes {
                return Err(AppError::TooLarge);
            }
            hash.update(&chunk);
            file.write_all(&chunk).await?;
        }
        if total == 0 {
            return Err(AppError::Invalid);
        }
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&staging, root.join(name)).await?;
        Ok(UploadedMediaFile {
            name: name.to_owned(),
            bytes: total,
            sha256: format!("{:x}", hash.finalize()),
        })
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    }
    result
}

/// Dépose un fichier média dans le dossier d'entrée (`input_dir`) via flux binaire.
pub async fn upload_file(
    State(s): State<AppState>,
    Path(name): Path<String>,
    _auth: ManageLibrariesAuth,
    body: Body,
) -> AppResult<(StatusCode, Json<UploadedMediaFile>)> {
    let uploaded =
        write_input_stream(&s.config.input_dir, &name, s.config.max_file_bytes, body).await?;
    Ok((StatusCode::CREATED, Json(uploaded)))
}

/// Dépose un fichier média dans le dossier d'entrée et crée immédiatement la tâche associée à la bibliothèque.
pub async fn upload_to_library(
    State(s): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    _auth: ManageLibrariesAuth,
    body: Body,
) -> AppResult<(StatusCode, Json<LibraryUploadResult>)> {
    repository::get_library(&s.pool, id).await?;
    let file =
        write_input_stream(&s.config.input_dir, &name, s.config.max_file_bytes, body).await?;
    let job = repository::insert_job_with_options(
        &s.pool,
        &file.name,
        None,
        &file.sha256,
        file.bytes as i64,
        Some(id),
        None,
    )
    .await?;
    s.notify_job_event(JobEventType::JobCreated, job.id, JobStatus::Queued, None);
    Ok((StatusCode::CREATED, Json(LibraryUploadResult { file, job })))
}

/// Logique partagée (HTTP et MCP) de scan du dossier d'entrée pour une bibliothèque donnée.
pub async fn scan_library_inner(
    s: &AppState,
    id: Uuid,
    req: LibraryScanRequest,
) -> AppResult<LibraryScanResult> {
    repository::get_library(&s.pool, id).await?;
    if let Some(worker) = req.target_worker_id
        && !repository::worker_exists(&s.pool, worker).await?
    {
        return Err(AppError::Invalid);
    }
    let files: Vec<MediaFile> =
        storage::list_inputs(&s.config.input_dir, s.config.max_file_bytes).await?;
    let scanned_files = files.len();
    let mut created_jobs = 0usize;
    let mut skipped_existing = 0usize;
    let mut created_files = Vec::new();
    for file in files {
        let path = storage::input_path(&s.config.input_dir, &file.name).await?;
        let (hash, bytes) = storage::fingerprint(&path, s.config.max_file_bytes).await?;
        if repository::has_library_file_job(&s.pool, id, &file.name, &hash).await? {
            skipped_existing += 1;
            continue;
        }
        if !req.dry_run {
            let job = repository::insert_job_with_options(
                &s.pool,
                &file.name,
                req.target_worker_id,
                &hash,
                bytes as i64,
                Some(id),
                req.flow_id.as_deref(),
            )
            .await?;
            s.notify_job_event(JobEventType::JobCreated, job.id, JobStatus::Queued, None);
        }
        created_jobs += 1;
        created_files.push(file.name);
    }
    Ok(LibraryScanResult {
        library_id: id,
        dry_run: req.dry_run,
        scanned_files,
        created_jobs,
        skipped_existing,
        created_files,
    })
}

/// Supprime un fichier média présent dans le dossier d'entrée (`input_dir`).
pub async fn delete_file(
    State(s): State<AppState>,
    Path(name): Path<String>,
    _auth: ManageLibrariesAuth,
) -> AppResult<StatusCode> {
    let path = storage::input_path(&s.config.input_dir, &name).await?;
    tokio::fs::remove_file(path).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Scanne les fichiers éligibles du dossier d'entrée et soumet ceux qui ne sont pas encore en file pour cette bibliothèque.
pub async fn scan(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ManageLibrariesAuth,
    payload: Option<Json<LibraryScanRequest>>,
) -> AppResult<Json<LibraryScanResult>> {
    let req = payload.map(|Json(req)| req).unwrap_or_default();
    Ok(Json(scan_library_inner(&s, id, req).await?))
}

/// Explore l'arborescence (dossiers et médias) d'un worker local ou distant en ligne.
pub async fn browse(
    State(s): State<AppState>,
    Query(q): Query<BrowseQuery>,
    _auth: ManageLibrariesAuth,
) -> AppResult<Json<DirectoryListing>> {
    let listing = match q.worker_id {
        Some(worker_id) => {
            let worker = repository::get_worker(&s.pool, worker_id).await?;
            if worker.is_integrated() || worker.remote_addr.as_deref() == Some("local") {
                filesystem_browser::browse_local(
                    worker.id,
                    q.path.as_deref(),
                    Some(&s.config.input_dir),
                )
                .await?
            } else {
                if !worker.is_online(chrono::Utc::now()) {
                    return Err(AppError::CentralUnavailable);
                }
                s.browse_broker
                    .request(worker.id, q.path, REMOTE_BROWSE_TIMEOUT)
                    .await?
            }
        }
        None => {
            filesystem_browser::browse_local(
                Uuid::nil(),
                q.path.as_deref(),
                Some(&s.config.input_dir),
            )
            .await?
        }
    };
    Ok(Json(listing))
}

/// Demande d'exploration en attente pour un worker distant authentifié.
pub async fn worker_pending_browse(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<WorkerPendingBrowseQuery>,
) -> AppResult<Json<Vec<WorkerBrowseTask>>> {
    crate::auth::authorize_worker(&s.pool, &headers).await?;
    let wait_ms = q.wait_ms.unwrap_or(0).min(MAX_BROWSE_WAIT_MS);
    let tasks = s
        .browse_broker
        .take_for_worker(q.worker_id, Duration::from_millis(wait_ms))
        .await;
    Ok(Json(tasks))
}

/// Réception du résultat d'exploration envoyé par un worker distant.
pub async fn worker_report_browse(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(report): Json<WorkerBrowseReport>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &headers).await?;
    let _ = s.browse_broker.complete(report).await;
    Ok(StatusCode::NO_CONTENT)
}
