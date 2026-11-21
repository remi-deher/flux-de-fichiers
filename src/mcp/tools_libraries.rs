//! Outils MCP pour les bibliothèques, fichiers d'entrée, tâches et diagnostics.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args, req_str};
use crate::{
    api::{
        AppState, ManageJobsAuth, ManageLibrariesAuth, ViewJobsAuth, ViewLibrariesAuth, jobs,
        libraries::{self, BrowseQuery, LibraryScanRequest},
        transfers,
    },
    auth::{self, Principal},
    domain::{
        JobBatchAction, JobBatchRequest, LibraryOutputPolicy, LibraryWorkerPath, LibraryWrite,
        NewJob, UpdateJobPriorityRequest,
    },
    flows::mask_definition,
    repository,
    tool_run::mask_log_secrets,
};
use axum::extract::{Json, Path, Query, State};
use forgearr_shared::Permission;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool::read_only(
            "list_libraries",
            "Liste les bibliothèques média configurées, leurs dossiers et règles d'automatisation.",
            empty_schema(),
        ),
        McpTool::read_only(
            "get_library",
            "Affiche le détail d'une bibliothèque média.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "browse_filesystem",
            "Parcourt les dossiers médias disponibles sur le central ou un worker.",
            obj_schema(
                json!({
                    "workerId": { "type": "string" },
                    "path": { "type": "string" }
                }),
                &[],
            ),
        ),
        McpTool::write(
            "create_library",
            "Crée une nouvelle bibliothèque média avec son flux et ses règles d'automatisation.",
            library_schema(&["name"]),
            false,
        ),
        McpTool::write(
            "update_library",
            "Modifie une bibliothèque média existante (flux, chemins, règles de scan et politique de sortie).",
            library_schema(&["id", "name"]),
            true,
        ),
        McpTool::destructive(
            "delete_library",
            "Supprime une bibliothèque média.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "scan_library",
            "Analyse les fichiers disponibles et lance le traitement des nouveaux médias.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "dryRun": { "type": "boolean" },
                    "targetWorkerId": { "type": "string" },
                    "flowId": { "type": "string" }
                }),
                &["id"],
            ),
            false,
        ),
        McpTool::read_only(
            "list_input_files",
            "Liste les fichiers médias en attente dans le dossier d'entrée.",
            empty_schema(),
        ),
        McpTool::destructive(
            "delete_input_file",
            "Supprime un fichier média du dossier d'entrée.",
            obj_schema(json!({ "name": { "type": "string" } }), &["name"]),
        ),
        McpTool::write(
            "create_job",
            "Lance le traitement d'un fichier existant (pour une bibliothèque ou un flux).",
            obj_schema(
                json!({
                    "sourceName": { "type": "string" },
                    "libraryId": { "type": "string" },
                    "flowId": { "type": "string" },
                    "targetWorkerId": { "type": "string" },
                    "priority": { "type": "integer", "minimum": -100, "maximum": 100 }
                }),
                &["sourceName"],
            ),
            false,
        ),
        McpTool::read_only(
            "list_jobs",
            "Liste les tâches récentes (jusqu'à 200).",
            obj_schema(json!({ "limit": { "type": "integer" } }), &[]),
        ),
        McpTool::read_only(
            "get_job",
            "Affiche l'état détaillé d'une tâche.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "get_job_steps",
            "Affiche le détail étape par étape d'une tâche.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "get_job_stats",
            "Retourne les statistiques globales et économies d'espace disque par bibliothèque.",
            empty_schema(),
        ),
        McpTool::read_only(
            "diagnose_job",
            "Agrège en un appel : fiche de la tâche, étapes nettoyées, version figée du flux (secrets masqués) et worker.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "retry_job",
            "Relance une tâche en échec après expiration de bail.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            false,
        ),
        McpTool::write(
            "cancel_job",
            "Annule activement une tâche en attente ou en cours d'exécution.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            true,
        ),
        McpTool::write(
            "update_job_priority",
            "Modifie la priorité d'une tâche en file (-100 à 100).",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "priority": { "type": "integer", "minimum": -100, "maximum": 100 }
                }),
                &["id", "priority"],
            ),
            true,
        ),
        McpTool::write(
            "batch_jobs",
            "Applique une action par lot (retry, cancel ou delete) à une sélection de tâches.",
            obj_schema(
                json!({
                    "ids": { "type": "array", "items": { "type": "string" } },
                    "action": { "type": "string", "enum": ["retry", "cancel", "delete"] }
                }),
                &["ids", "action"],
            ),
            false,
        ),
        McpTool::destructive(
            "delete_job",
            "Supprime une tâche qui n'est pas en cours d'exécution.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::destructive(
            "delete_job_output",
            "Supprime le fichier produit par une tâche terminée.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
    ]
}

fn library_schema(required: &[&str]) -> Value {
    obj_schema(
        json!({
            "id": { "type": "string" },
            "name": { "type": "string" },
            "flowId": { "type": "string" },
            "workers": { "type": "array" },
            "scanIntervalMinutes": { "type": "integer", "minimum": 0 },
            "includeExtensions": { "type": "array", "items": { "type": "string" } },
            "excludePatterns": { "type": "array", "items": { "type": "string" } },
            "minFileBytes": { "type": "integer", "minimum": 0 },
            "outputPolicy": { "type": "string", "enum": ["output_dir", "replace_with_backup", "replace_in_place"] },
            "defaultPriority": { "type": "integer", "minimum": forgearr_shared::jobs::JOB_PRIORITY_MIN, "maximum": forgearr_shared::jobs::JOB_PRIORITY_MAX }
        }),
        required,
    )
}

#[derive(Deserialize)]
struct IdArg {
    id: Uuid,
}

#[derive(Deserialize)]
struct LibraryWriteArgs {
    #[serde(default)]
    id: Option<Uuid>,
    name: String,
    #[serde(default, alias = "flowId")]
    flow_id: Option<String>,
    #[serde(default)]
    workers: Vec<LibraryWorkerPath>,
    #[serde(default, alias = "transferWorkers")]
    transfer_workers: Vec<Uuid>,
    #[serde(default, alias = "scanIntervalMinutes")]
    scan_interval_minutes: i32,
    #[serde(default, alias = "includeExtensions")]
    include_extensions: Vec<String>,
    #[serde(default, alias = "excludePatterns")]
    exclude_patterns: Vec<String>,
    #[serde(default, alias = "minFileBytes")]
    min_file_bytes: u64,
    #[serde(default, alias = "outputPolicy")]
    output_policy: LibraryOutputPolicy,
    #[serde(
        default = "forgearr_shared::jobs::default_library_job_priority",
        alias = "defaultPriority"
    )]
    default_priority: i32,
}

impl From<LibraryWriteArgs> for LibraryWrite {
    fn from(value: LibraryWriteArgs) -> Self {
        Self {
            name: value.name,
            flow_id: value.flow_id,
            workers: value.workers,
            transfer_workers: value.transfer_workers,
            scan_interval_minutes: value.scan_interval_minutes,
            include_extensions: value.include_extensions,
            exclude_patterns: value.exclude_patterns,
            min_file_bytes: value.min_file_bytes,
            output_policy: value.output_policy,
            default_priority: value.default_priority,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanLibraryArgs {
    id: Uuid,
    #[serde(default)]
    dry_run: bool,
    target_worker_id: Option<Uuid>,
    flow_id: Option<String>,
}

#[derive(Deserialize)]
struct ListJobsArgs {
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct UpdatePriorityArgs {
    id: Uuid,
    priority: i32,
}

#[derive(Deserialize)]
struct BatchJobsArgs {
    ids: Vec<Uuid>,
    action: JobBatchAction,
}

async fn run_diagnose_job(
    id: Uuid,
    state: &AppState,
    principal: &Principal,
) -> Result<McpToolResult, crate::error::AppError> {
    auth::require(principal, Permission::ViewJobs)?;
    let job = repository::get_job(&state.pool, id).await?;
    let mut steps = repository::list_job_steps(&state.pool, id).await?;
    for step in &mut steps {
        if let Some(ref msg) = step.message {
            step.message = Some(mask_log_secrets(msg));
        }
        if let Some(ref excerpt) = step.output_excerpt {
            step.output_excerpt = Some(mask_log_secrets(excerpt));
        }
    }
    let frozen_flow = match (job.flow_id.as_deref(), job.flow_version) {
        (Some(flow_id), Some(version)) => {
            match repository::get_flow_version(&state.pool, flow_id, version).await {
                Ok(mut record) => {
                    record.definition = mask_definition(&record.definition);
                    Some(record)
                }
                Err(_) => None,
            }
        }
        _ => None,
    };
    let worker = match job.worker_id {
        Some(worker_id) => repository::get_worker(&state.pool, worker_id).await.ok(),
        None => None,
    };
    let library = match job.library_id {
        Some(lib_id) => repository::get_library(&state.pool, lib_id).await.ok(),
        None => None,
    };

    Ok(McpToolResult::json(&json!({
        "job": job,
        "steps": steps,
        "frozenFlow": frozen_flow,
        "worker": worker,
        "library": library,
    })))
}

pub async fn call(
    name: &str,
    args: Value,
    state: &AppState,
    principal: &Principal,
) -> Option<McpToolResult> {
    let result = match name {
        "list_libraries" => {
            libraries::list(State(state.clone()), ViewLibrariesAuth(principal.clone()))
                .await
                .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_library" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::get(
                State(state.clone()),
                Path(id),
                ViewLibrariesAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "browse_filesystem" => {
            let query: BrowseQuery = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::browse(
                State(state.clone()),
                Query(query),
                ManageLibrariesAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "create_library" => {
            let parsed: LibraryWriteArgs = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::create(
                State(state.clone()),
                ManageLibrariesAuth(principal.clone()),
                Json(parsed.into()),
            )
            .await
            .map(|(_, Json(v))| McpToolResult::json(&v))
        }
        "update_library" => {
            let parsed: LibraryWriteArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            let Some(id) = parsed.id else {
                return Some(McpToolResult::error("paramètre requis manquant : id"));
            };
            libraries::update(
                State(state.clone()),
                Path(id),
                ManageLibrariesAuth(principal.clone()),
                Json(parsed.into()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "delete_library" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::delete(
                State(state.clone()),
                Path(id),
                ManageLibrariesAuth(principal.clone()),
            )
            .await
            .map(|_| McpToolResult::json(&json!({ "id": id, "deleted": true })))
        }
        "scan_library" => {
            if let Err(err) = auth::require(principal, Permission::ManageLibraries)
                .and_then(|()| auth::require(principal, Permission::ManageJobs))
            {
                return Some(McpToolResult::error(format!("{err:?}")));
            }
            let parsed: ScanLibraryArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::scan_library_inner(
                state,
                parsed.id,
                LibraryScanRequest {
                    dry_run: parsed.dry_run,
                    target_worker_id: parsed.target_worker_id,
                    flow_id: parsed.flow_id,
                },
            )
            .await
            .map(|v| McpToolResult::json(&v))
        }
        "list_input_files" => {
            jobs::files(State(state.clone()), ViewLibrariesAuth(principal.clone()))
                .await
                .map(|Json(v)| McpToolResult::json(&v))
        }
        "delete_input_file" => {
            let file_name = match req_str(&args, "name") {
                Ok(n) => n.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::delete_file(
                State(state.clone()),
                Path(file_name.clone()),
                ManageLibrariesAuth(principal.clone()),
            )
            .await
            .map(|_| McpToolResult::json(&json!({ "name": file_name, "deleted": true })))
        }
        "create_job" => {
            let req: NewJob = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::create(
                State(state.clone()),
                ManageJobsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|(_, Json(v))| McpToolResult::json(&v))
        }
        "list_jobs" => {
            if let Err(err) = auth::require(principal, Permission::ViewJobs) {
                return Some(McpToolResult::error(format!("{err:?}")));
            }
            let parsed: ListJobsArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            let limit = parsed.limit.unwrap_or(50).clamp(1, 200);
            repository::list_jobs(&state.pool, None, limit)
                .await
                .map(|v| McpToolResult::json(&v))
        }
        "get_job" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::get_job(
                State(state.clone()),
                Path(id),
                ViewJobsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_job_steps" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::steps(
                State(state.clone()),
                Path(id),
                ViewJobsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_job_stats" => jobs::stats(State(state.clone()), ViewJobsAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "diagnose_job" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            run_diagnose_job(id, state, principal).await
        }
        "retry_job" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::retry(
                State(state.clone()),
                Path(id),
                ManageJobsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "cancel_job" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::cancel(
                State(state.clone()),
                Path(id),
                ManageJobsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "update_job_priority" => {
            let parsed: UpdatePriorityArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::update_priority(
                State(state.clone()),
                Path(parsed.id),
                ManageJobsAuth(principal.clone()),
                Json(UpdateJobPriorityRequest {
                    priority: parsed.priority,
                }),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "batch_jobs" => {
            let parsed: BatchJobsArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::batch(
                State(state.clone()),
                ManageJobsAuth(principal.clone()),
                Json(JobBatchRequest {
                    ids: parsed.ids,
                    action: parsed.action,
                }),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "delete_job" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::delete_job(
                State(state.clone()),
                Path(id),
                ManageJobsAuth(principal.clone()),
            )
            .await
            .map(|_| McpToolResult::json(&json!({ "id": id, "deleted": true })))
        }
        "delete_job_output" => {
            let IdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            transfers::delete_output(
                State(state.clone()),
                Path(id),
                ManageJobsAuth(principal.clone()),
            )
            .await
            .map(|_| McpToolResult::json(&json!({ "id": id, "outputDeleted": true })))
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|err| McpToolResult::error(format!("{err:?}"))))
}
