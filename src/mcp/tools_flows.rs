//! Outils MCP pour la gestion des flux de traitement et de leurs blocs.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args, req_str};
use crate::{
    api::{AdminAuth, AppState, ViewFlowsAuth, flows},
    auth::Principal,
    domain::UpdateFlowRequest,
};
use axum::extract::{Json, Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "list_blocks",
            description: "Liste le catalogue des blocs de flux disponibles avec leurs champs et ports.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "list_flows",
            description: "Liste tous les flux configurés (les secrets webhook restent masqués).",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "get_flow",
            description: "Lit la définition détaillée d'un flux par son identifiant.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "save_flow",
            description: "Crée ou met à jour un flux (nouvelle version immuable).",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "isDefault": { "type": "boolean" },
                    "definition": { "type": "object" }
                }),
                &["id", "name", "definition"],
            ),
        },
        McpTool {
            name: "delete_flow",
            description: "Supprime un flux par son identifiant.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "list_flow_versions",
            description: "Liste les versions figées d'un flux.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "get_flow_version",
            description: "Lit une version figée spécifique d'un flux.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "version": { "type": "integer" }
                }),
                &["id", "version"],
            ),
        },
        McpTool {
            name: "validate_flow",
            description: "Valide un graphe de flux (blocs connus, champs requis, absence de cycle) avant sauvegarde.",
            input_schema: obj_schema(
                json!({ "definition": { "type": "object" } }),
                &["definition"],
            ),
        },
        McpTool {
            name: "restore_flow_version",
            description: "Restaure une version figée d'un flux en tant que nouvelle version courante.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "version": { "type": "integer" }
                }),
                &["id", "version"],
            ),
        },
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveFlowArgs {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    file_mask: Option<String>,
    #[serde(default)]
    is_default: bool,
    definition: Value,
}

#[derive(Deserialize)]
struct FlowVersionArgs {
    id: String,
    version: i64,
}

pub async fn call(
    name: &str,
    args: Value,
    state: &AppState,
    principal: &Principal,
) -> Option<McpToolResult> {
    let result = match name {
        "list_blocks" => flows::list_blocks(State(state.clone()), ViewFlowsAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "list_flows" => flows::list_flows(State(state.clone()), ViewFlowsAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "get_flow" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::get_flow(
                State(state.clone()),
                Path(id),
                ViewFlowsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "save_flow" => {
            let parsed: SaveFlowArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            let definition = match parsed.definition {
                Value::String(s) => s,
                other => other.to_string(),
            };
            let req = UpdateFlowRequest {
                name: parsed.name,
                description: parsed.description,
                file_mask: parsed.file_mask.unwrap_or_default(),
                definition,
                is_default: parsed.is_default,
            };
            flows::save_flow(
                State(state.clone()),
                Path(parsed.id),
                AdminAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|(_, _, Json(v))| McpToolResult::json(&v))
        }
        "delete_flow" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::delete_flow(
                State(state.clone()),
                Path(id.clone()),
                AdminAuth(principal.clone()),
            )
            .await
            .map(|_| McpToolResult::json(&json!({ "id": id, "deleted": true })))
        }
        "list_flow_versions" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::list_versions(
                State(state.clone()),
                Path(id),
                ViewFlowsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_flow_version" => {
            let parsed: FlowVersionArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::get_version(
                State(state.clone()),
                Path((parsed.id, parsed.version)),
                ViewFlowsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "validate_flow" => {
            let req: flows::ValidateFlowRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::validate_flow(
                State(state.clone()),
                ViewFlowsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "restore_flow_version" => {
            let parsed: FlowVersionArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::restore_version(
                State(state.clone()),
                Path((parsed.id, parsed.version)),
                AdminAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|err| McpToolResult::error(format!("{err:?}"))))
}
