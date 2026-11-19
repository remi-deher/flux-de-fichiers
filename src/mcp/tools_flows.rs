//! Outils MCP pour la gestion, la simulation et l'analyse différentielle des flux de traitement.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args, req_str};
use crate::{
    api::{AppState, ManageFlowsAuth, ViewFlowsAuth, flows},
    auth::{self, Principal},
    domain::UpdateFlowRequest,
    repository,
};
use axum::extract::{Json, Path, State};
use forgearr_shared::{
    Permission,
    flow_analysis::{SimulationPreset, diff_flow_graphs, simulate_flow_basic},
    flows::FlowGraph,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool::read_only(
            "list_blocks",
            "Liste le catalogue des blocs de flux disponibles avec leurs champs et ports.",
            empty_schema(),
        ),
        McpTool::read_only(
            "list_flows",
            "Liste tous les flux configurés (les secrets webhook restent masqués).",
            empty_schema(),
        ),
        McpTool::read_only(
            "get_flow",
            "Lit la définition détaillée d'un flux par son identifiant.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "save_flow",
            "Crée ou met à jour un flux (nouvelle version immuable).",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "fileMask": { "type": "string" },
                    "isDefault": { "type": "boolean" },
                    "definition": { "type": "object" }
                }),
                &["id", "name", "definition"],
            ),
            true,
        ),
        McpTool::destructive(
            "delete_flow",
            "Supprime un flux par son identifiant.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "list_flow_versions",
            "Liste les versions figées d'un flux.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "get_flow_version",
            "Lit une version figée spécifique d'un flux.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "version": { "type": "integer" }
                }),
                &["id", "version"],
            ),
        ),
        McpTool::read_only(
            "validate_flow",
            "Valide un graphe de flux (blocs connus, champs requis, absence de cycle) avant sauvegarde.",
            obj_schema(
                json!({ "definition": { "type": "object" } }),
                &["definition"],
            ),
        ),
        McpTool::read_only(
            "simulate_flow",
            "Simule à sec le parcours d'un flux (fourni en JSON ou via flowId) sur un profil de sonde avant de l'enregistrer.",
            obj_schema(
                json!({
                    "flowId": { "type": "string" },
                    "definition": { "type": "object" },
                    "preset": { "type": "string", "enum": ["structured_tracks", "document_metadata", "minimal"] },
                    "fileName": { "type": "string" },
                    "probeJson": { "type": ["object", "string"] }
                }),
                &[],
            ),
        ),
        McpTool::read_only(
            "diff_flow_versions",
            "Compare deux versions figées d'un flux (ou les deux dernières par défaut) et résume nœuds/liaisons modifiés.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "fromVersion": { "type": "integer" },
                    "toVersion": { "type": "integer" }
                }),
                &["id"],
            ),
        ),
        McpTool::write(
            "restore_flow_version",
            "Restaure une version figée d'un flux en tant que nouvelle version courante.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "version": { "type": "integer" }
                }),
                &["id", "version"],
            ),
            false,
        ),
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimulateFlowArgs {
    #[serde(default)]
    flow_id: Option<String>,
    #[serde(default)]
    definition: Option<Value>,
    #[serde(default)]
    preset: Option<SimulationPreset>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    probe_json: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffFlowVersionsArgs {
    id: String,
    #[serde(default)]
    from_version: Option<i64>,
    #[serde(default)]
    to_version: Option<i64>,
}

async fn run_simulate_flow(
    args: SimulateFlowArgs,
    state: &AppState,
    principal: &Principal,
) -> Result<McpToolResult, crate::error::AppError> {
    auth::require(principal, Permission::ViewFlows)?;
    let graph = match (args.definition, args.flow_id.as_deref()) {
        (Some(def), _) => match def {
            Value::String(raw) => FlowGraph::parse(&raw),
            other => serde_json::from_value(other).map_err(|_| crate::error::AppError::Invalid)?,
        },
        (None, Some(id)) => {
            let flow = repository::get_flow(&state.pool, id.trim()).await?;
            FlowGraph::parse(&flow.definition)
        }
        (None, None) => {
            return Ok(McpToolResult::error(
                "paramètre requis : fournissez `definition` ou `flowId`",
            ));
        }
    };

    let mut input = args
        .preset
        .unwrap_or(SimulationPreset::StructuredTracks)
        .input();
    if let Some(name) = args
        .file_name
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
    {
        input.file_name = name;
    }
    if let Some(probe) = args.probe_json {
        input.probe_json = match probe {
            Value::String(s) => s,
            other => other.to_string(),
        };
    }

    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let catalog = state.catalog_cache.blocks(&roots).await;
    let report = simulate_flow_basic(&graph, &catalog, &input);
    Ok(McpToolResult::json(&report))
}

async fn run_diff_flow_versions(
    args: DiffFlowVersionsArgs,
    state: &AppState,
    principal: &Principal,
) -> Result<McpToolResult, crate::error::AppError> {
    auth::require(principal, Permission::ViewFlows)?;
    let versions = repository::list_flow_versions(&state.pool, &args.id).await?;
    let to_ver = match args.to_version {
        Some(v) => v,
        None => versions
            .first()
            .map(|v| v.version)
            .ok_or(crate::error::AppError::NotFound)?,
    };
    let from_ver = match args.from_version {
        Some(v) => v,
        None => versions
            .iter()
            .find(|v| v.version < to_ver)
            .map(|v| v.version)
            .unwrap_or(to_ver),
    };

    let before_record = repository::get_flow_version(&state.pool, &args.id, from_ver).await?;
    let after_record = repository::get_flow_version(&state.pool, &args.id, to_ver).await?;
    let diff = diff_flow_graphs(
        &FlowGraph::parse(&before_record.definition),
        &FlowGraph::parse(&after_record.definition),
    );
    Ok(McpToolResult::json(&json!({
        "flowId": args.id,
        "fromVersion": from_ver,
        "toVersion": to_ver,
        "nameChanged": before_record.name != after_record.name,
        "fileMaskChanged": before_record.file_mask != after_record.file_mask,
        "diff": diff,
    })))
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
                ManageFlowsAuth(principal.clone()),
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
                ManageFlowsAuth(principal.clone()),
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
        "simulate_flow" => {
            let parsed: SimulateFlowArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            run_simulate_flow(parsed, state, principal).await
        }
        "diff_flow_versions" => {
            let parsed: DiffFlowVersionsArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            run_diff_flow_versions(parsed, state, principal).await
        }
        "restore_flow_version" => {
            let parsed: FlowVersionArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            flows::restore_version(
                State(state.clone()),
                Path((parsed.id, parsed.version)),
                ManageFlowsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|err| McpToolResult::error(format!("{err:?}"))))
}
