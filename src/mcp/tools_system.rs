//! Outils MCP pour l'état du système, les réglages, les workers et leur enrôlement.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args};
use crate::{
    api::{
        AdminAuth, AppState, ManageWorkersAuth, ViewOverviewAuth, ViewWorkersAuth, diagnostics,
        installation, jobs, system,
    },
    auth::{self, Principal},
};
use axum::{
    body::Bytes,
    extract::{Json, Path, State},
    http::HeaderMap,
};
use forgearr_shared::{
    Permission,
    diagnostics::DiagnosticsRequest,
    system::UpdateSystemSettingsRequest,
    workers::{UpdateWorkerSettingsRequest, WorkerSchedule},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool::read_only(
            "get_system_status",
            "Lit la configuration et l'état du central (sans secrets).",
            empty_schema(),
        ),
        McpTool::read_only(
            "get_instance_status",
            "Lit le rôle, la version et l'état d'amorçage de l'instance.",
            empty_schema(),
        ),
        McpTool::write(
            "update_system_settings",
            "Met à jour les réglages réseau, courriel et sécurité du central.",
            obj_schema(
                json!({
                    "webOrigin": { "type": ["string", "null"] },
                    "sessionTtlSeconds": { "type": "integer" },
                    "smtpEnabled": { "type": "boolean" },
                    "allowPrivatePluginHosts": { "type": "boolean" },
                    "blockPrivateIntegrationHosts": { "type": "boolean" },
                    "checkUpdatesEnabled": { "type": "boolean" }
                }),
                &[],
            ),
            true,
        ),
        McpTool::read_only(
            "get_system_hardware",
            "Lit les capacités matérielles détectées sur l'hôte central.",
            empty_schema(),
        ),
        McpTool::read_only(
            "run_diagnostics",
            "Exécute le diagnostic complet du central (base, stockage, réseau, catalogue).",
            obj_schema(json!({ "browserOrigin": { "type": "string" } }), &[]),
        ),
        McpTool::read_only(
            "list_workers",
            "Liste les workers enregistrés, leur concurrence, plage horaire et télémétrie.",
            empty_schema(),
        ),
        McpTool::write(
            "update_worker_settings",
            "Configure la concurrence maximale (1 à 16), les places CPU/GPU (0 à 16, absentes : concurrence commune) et la fenêtre horaire d'un worker.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "maxConcurrency": { "type": "integer", "minimum": 1, "maximum": 16 },
                    "maxCpuJobs": { "type": ["integer", "null"], "minimum": 0, "maximum": 16 },
                    "maxGpuJobs": { "type": ["integer", "null"], "minimum": 0, "maximum": 16 },
                    "schedule": { "type": ["object", "null"] }
                }),
                &["id", "maxConcurrency"],
            ),
            true,
        ),
        McpTool::destructive(
            "delete_worker",
            "Supprime un worker enregistré par son UUID.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "list_enrollment_requests",
            "Liste la fenêtre d'enrôlement et les demandes de workers en attente d'approbation.",
            empty_schema(),
        ),
        McpTool::write(
            "set_enrollment_window",
            "Ouvre (pour 15 minutes) ou ferme la fenêtre d'acceptation des demandes d'enrôlement.",
            obj_schema(json!({ "open": { "type": "boolean" } }), &["open"]),
            true,
        ),
        McpTool::write(
            "approve_enrollment_request",
            "Approuve une demande d'enrôlement de worker en attente.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            false,
        ),
        McpTool::write(
            "reject_enrollment_request",
            "Refuse une demande d'enrôlement de worker en attente.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            false,
        ),
    ]
}

#[derive(Deserialize)]
struct UuidIdArg {
    id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunDiagnosticsArgs {
    #[serde(default)]
    browser_origin: String,
}

#[derive(Deserialize)]
struct UpdateWorkerArgs {
    id: Uuid,
    #[serde(alias = "maxConcurrency")]
    max_concurrency: i32,
    #[serde(default, alias = "maxCpuJobs")]
    max_cpu_jobs: Option<i32>,
    #[serde(default, alias = "maxGpuJobs")]
    max_gpu_jobs: Option<i32>,
    #[serde(default)]
    schedule: Option<WorkerSchedule>,
}

#[derive(Deserialize)]
struct EnrollmentWindowArgs {
    open: bool,
}

pub async fn call(
    name: &str,
    args: Value,
    state: &AppState,
    principal: &Principal,
) -> Option<McpToolResult> {
    let result = match name {
        "get_system_status" => system::status(State(state.clone()), AdminAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "get_instance_status" => {
            if let Err(err) = auth::require(principal, Permission::ViewOverview) {
                return Some(McpToolResult::error(format!("{err:?}")));
            }
            installation::status(State(state.clone()))
                .await
                .map(|Json(v)| McpToolResult::json(&v))
        }
        "update_system_settings" => {
            let req: UpdateSystemSettingsRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            system::update_settings(
                State(state.clone()),
                AdminAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_system_hardware" => {
            let Json(v) =
                system::hardware(State(state.clone()), ViewOverviewAuth(principal.clone())).await;
            Ok(McpToolResult::json(&v))
        }
        "run_diagnostics" => {
            let parsed: RunDiagnosticsArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            diagnostics::diagnostics(
                State(state.clone()),
                HeaderMap::new(),
                AdminAuth(principal.clone()),
                Json(DiagnosticsRequest {
                    browser_origin: parsed.browser_origin,
                }),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "list_workers" => jobs::workers(State(state.clone()), ViewWorkersAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "update_worker_settings" => {
            let parsed: UpdateWorkerArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::update_worker_settings(
                State(state.clone()),
                Path(parsed.id),
                ManageWorkersAuth(principal.clone()),
                Json(UpdateWorkerSettingsRequest {
                    max_concurrency: parsed.max_concurrency,
                    max_cpu_jobs: parsed.max_cpu_jobs,
                    max_gpu_jobs: parsed.max_gpu_jobs,
                    schedule: parsed.schedule,
                }),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "delete_worker" => {
            let UuidIdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            jobs::delete_worker(
                State(state.clone()),
                Path(id),
                ManageWorkersAuth(principal.clone()),
            )
            .await
            .map(|_| McpToolResult::json(&json!({ "id": id, "deleted": true })))
        }
        "list_enrollment_requests" => installation::list_enrollment_requests(
            State(state.clone()),
            ViewWorkersAuth(principal.clone()),
        )
        .await
        .map(|Json(v)| McpToolResult::json(&v)),
        "set_enrollment_window" => {
            let EnrollmentWindowArgs { open } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            let body = match serde_json::to_vec(&json!({ "open": open })) {
                Ok(bytes) => Bytes::from(bytes),
                Err(err) => return Some(McpToolResult::error(err.to_string())),
            };
            installation::set_enrollment_window(
                State(state.clone()),
                ManageWorkersAuth(principal.clone()),
                body,
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "approve_enrollment_request" => {
            let UuidIdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            installation::approve_enrollment_request(
                State(state.clone()),
                Path(id),
                ManageWorkersAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "reject_enrollment_request" => {
            let UuidIdArg { id } = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            installation::reject_enrollment_request(
                State(state.clone()),
                Path(id),
                ManageWorkersAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|err| McpToolResult::error(format!("{err:?}"))))
}
