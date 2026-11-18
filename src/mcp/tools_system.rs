//! Outils MCP pour l'état du système, les réglages et les workers.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args};
use crate::{
    api::{
        AdminAuth, AppState, ManageWorkersAuth, ViewOverviewAuth, ViewWorkersAuth, diagnostics,
        installation, jobs, system,
    },
    auth::{self, Principal},
};
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
};
use flux_shared::{
    Permission, diagnostics::DiagnosticsRequest, system::UpdateSystemSettingsRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "get_system_status",
            description: "Lit la configuration et l'état du central (sans secrets).",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "get_instance_status",
            description: "Lit le rôle, la version et l'état d'amorçage de l'instance.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "update_system_settings",
            description: "Met à jour les réglages système du central.",
            input_schema: obj_schema(
                json!({
                    "instanceName": { "type": "string" },
                    "maxFileBytes": { "type": "integer" },
                    "localWorker": { "type": "boolean" },
                    "allowPrivateRepoHosts": { "type": "boolean" },
                    "requireImmutablePluginReleases": { "type": "boolean" }
                }),
                &[],
            ),
        },
        McpTool {
            name: "get_system_hardware",
            description: "Lit les capacités matérielles détectées sur l'hôte central.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "run_diagnostics",
            description: "Exécute le diagnostic complet du central (base, stockage, réseau, catalogue).",
            input_schema: obj_schema(json!({ "browserOrigin": { "type": "string" } }), &[]),
        },
        McpTool {
            name: "list_workers",
            description: "Liste les workers enregistrés et leur état de présence.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "delete_worker",
            description: "Supprime un worker enregistré par son UUID.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
    ]
}

#[derive(Deserialize)]
struct WorkerIdArg {
    id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunDiagnosticsArgs {
    #[serde(default)]
    browser_origin: String,
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
        "delete_worker" => {
            let WorkerIdArg { id } = match parse_args(args) {
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
        _ => return None,
    };
    Some(result.unwrap_or_else(|err| McpToolResult::error(format!("{err:?}"))))
}
