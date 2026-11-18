//! Outils MCP pour les bibliothèques, fichiers d'entrée et tâches de traitement.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args, req_str};
use crate::{
    api::{
        AppState, ManageJobsAuth, ManageLibrariesAuth, ViewJobsAuth, ViewLibrariesAuth, jobs,
        libraries::{self, LibraryScanRequest},
        transfers,
    },
    auth::{self, Principal},
    domain::{LibraryWrite, NewJob},
    repository,
};
use axum::extract::{Json, Path, State};
use flux_shared::Permission;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "list_libraries",
            description: "Liste les bibliothèques média configurées et leurs dossiers.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "get_library",
            description: "Affiche le détail d'une bibliothèque média.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "create_library",
            description: "Crée une nouvelle bibliothèque média et lui associe un flux.",
            input_schema: obj_schema(
                json!({
                    "name": { "type": "string" },
                    "flowId": { "type": "string" },
                    "workers": { "type": "array" }
                }),
                &["name"],
            ),
        },
        McpTool {
            name: "update_library",
            description: "Modifie une bibliothèque média existante.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "flowId": { "type": "string" },
                    "workers": { "type": "array" }
                }),
                &["id", "name"],
            ),
        },
        McpTool {
            name: "delete_library",
            description: "Supprime une bibliothèque média.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "scan_library",
            description: "Analyse les fichiers disponibles et lance le traitement des nouveaux médias.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "dryRun": { "type": "boolean" },
                    "targetWorkerId": { "type": "string" },
                    "flowId": { "type": "string" }
                }),
                &["id"],
            ),
        },
        McpTool {
            name: "list_input_files",
            description: "Liste les fichiers médias en attente dans le dossier d'entrée.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "delete_input_file",
            description: "Supprime un fichier média du dossier d'entrée.",
            input_schema: obj_schema(json!({ "name": { "type": "string" } }), &["name"]),
        },
        McpTool {
            name: "create_job",
            description: "Lance le traitement d'un fichier existant (pour une bibliothèque ou un flux).",
            input_schema: obj_schema(
                json!({
                    "sourceName": { "type": "string" },
                    "libraryId": { "type": "string" },
                    "flowId": { "type": "string" },
                    "targetWorkerId": { "type": "string" }
                }),
                &["sourceName"],
            ),
        },
        McpTool {
            name: "list_jobs",
            description: "Liste les tâches récentes (jusqu'à 50).",
            input_schema: obj_schema(json!({ "limit": { "type": "integer" } }), &[]),
        },
        McpTool {
            name: "get_job",
            description: "Affiche l'état détaillé d'une tâche.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "get_job_steps",
            description: "Affiche le détail étape par étape d'une tâche (utile pour diagnostiquer un échec).",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "retry_job",
            description: "Relance une tâche en échec.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "delete_job",
            description: "Annule ou supprime une tâche qui n'est pas en cours.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "delete_job_output",
            description: "Supprime le fichier produit par une tâche terminée.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
    ]
}

#[derive(Deserialize)]
struct IdArg {
    id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateLibraryArgs {
    id: Uuid,
    name: String,
    #[serde(default)]
    flow_id: Option<String>,
    #[serde(default)]
    workers: Vec<crate::domain::LibraryWorkerPath>,
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
        "create_library" => {
            let req: LibraryWrite = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            libraries::create(
                State(state.clone()),
                ManageLibrariesAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|(_, Json(v))| McpToolResult::json(&v))
        }
        "update_library" => {
            let parsed: UpdateLibraryArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            let write = LibraryWrite {
                name: parsed.name,
                flow_id: parsed.flow_id,
                workers: parsed.workers,
            };
            libraries::update(
                State(state.clone()),
                Path(parsed.id),
                ManageLibrariesAuth(principal.clone()),
                Json(write),
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
