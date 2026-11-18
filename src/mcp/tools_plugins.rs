//! Outils MCP pour les dépôts de plugins, le catalogue, les outils, packs et extensions locales.

use super::protocol::{McpTool, McpToolResult, empty_schema, obj_schema, parse_args, req_str};
use crate::{
    api::{AnyAuth, AppState, ManagePluginsAuth, ViewPluginsAuth, local, plugins},
    auth::Principal,
    plugins::CreateRepositoryRequest,
};
use axum::extract::{Json, Path, State};
use flux_shared::local::{
    CreateBlockTemplateRequest, CreateFlowTemplateRequest, CreateLocalPackRequest,
    UploadLocalResourceRequest, UpsertLocalScriptRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "list_repositories",
            description: "Liste les dépôts de plugins configurés.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "get_repository",
            description: "Lit le détail d'un dépôt de plugins et ses entrées synchronisées.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "add_repository",
            description: "Ajoute un dépôt de plugins distant (URL vers index.json) et le synchronise.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "indexUrl": { "type": "string" }
                }),
                &["name", "indexUrl"],
            ),
        },
        McpTool {
            name: "sync_repository",
            description: "Synchronise immédiatement un dépôt de plugins.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "toggle_repository",
            description: "Active ou désactive un dépôt de plugins.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "delete_repository",
            description: "Supprime un dépôt de plugins non officiel.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "list_catalog",
            description: "Liste les plugins, packs, flux modèles et scripts du catalogue synchronisé.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "get_plugin",
            description: "Lit le détail d'un plugin (blocs exposés, dépendances, paramètres).",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "get_plugin_settings",
            description: "Lit les réglages d'un plugin (les secrets sont masqués).",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "save_plugin_settings",
            description: "Enregistre les réglages d'un plugin (les secrets sont chiffrés en base).",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "settings": { "type": "object" }
                }),
                &["id", "settings"],
            ),
        },
        McpTool {
            name: "list_tools",
            description: "Liste les applications/outils distribués et leur état d'installation.",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "get_tool",
            description: "Lit le détail d'une application distribuée (artefacts, SHA-256, compatibilité).",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "install_tool",
            description: "Télécharge, vérifie (SHA-256) et installe une application distribuée dans le cache.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "get_pack",
            description: "Lit le détail d'un pack (composants et plan d'installation).",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "install_pack",
            description: "Installe un pack du catalogue et ses outils requis.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        McpTool {
            name: "list_local_catalog",
            description: "Liste le catalogue local (scripts, ressources, modèles de blocs/flux, packs).",
            input_schema: empty_schema(),
        },
        McpTool {
            name: "upload_local_resource",
            description: "Dépose une ressource locale (contenu encodé en base64).",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "filename": { "type": "string" },
                    "contentBase64": { "type": "string" }
                }),
                &["name", "filename", "contentBase64"],
            ),
        },
        McpTool {
            name: "upsert_local_script",
            description: "Crée ou met à jour un script local exécutable dans les flux.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "interpreter": { "type": "string" },
                    "content": { "type": "string" }
                }),
                &["name", "interpreter", "content"],
            ),
        },
        McpTool {
            name: "create_local_block_template",
            description: "Enregistre un modèle de bloc local.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "block": { "type": "object" }
                }),
                &["name", "block"],
            ),
        },
        McpTool {
            name: "create_local_flow_template",
            description: "Enregistre un modèle de flux local.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "definition": { "type": "object" }
                }),
                &["name", "definition"],
            ),
        },
        McpTool {
            name: "create_local_pack",
            description: "Crée un pack local regroupant des scripts, ressources et modèles.",
            input_schema: obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "components": { "type": "array" }
                }),
                &["name", "components"],
            ),
        },
        McpTool {
            name: "delete_local_item",
            description: "Supprime un élément du catalogue local par son identifiant.",
            input_schema: obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
    ]
}

#[derive(Deserialize)]
struct SaveSettingsArgs {
    id: String,
    settings: Value,
}

pub async fn call(
    name: &str,
    args: Value,
    state: &AppState,
    principal: &Principal,
) -> Option<McpToolResult> {
    let result = match name {
        "list_repositories" => {
            plugins::list_repositories(State(state.clone()), ViewPluginsAuth(principal.clone()))
                .await
                .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_repository" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::repository_detail(
                State(state.clone()),
                Path(id),
                ViewPluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "add_repository" => {
            let req: CreateRepositoryRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::add_repository(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "sync_repository" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::sync_repository(
                State(state.clone()),
                Path(id),
                ManagePluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "toggle_repository" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::toggle_repository(
                State(state.clone()),
                Path(id),
                ManagePluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "delete_repository" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::delete_repository(
                State(state.clone()),
                Path(id),
                ManagePluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "list_catalog" => {
            plugins::catalog(State(state.clone()), ViewPluginsAuth(principal.clone()))
                .await
                .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_plugin" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::plugin_detail(
                State(state.clone()),
                Path(id),
                ViewPluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_plugin_settings" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::get_settings(
                State(state.clone()),
                Path(id),
                ManagePluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "save_plugin_settings" => {
            let parsed: SaveSettingsArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::save_settings(
                State(state.clone()),
                Path(parsed.id),
                ManagePluginsAuth(principal.clone()),
                Json(parsed.settings),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "list_tools" => plugins::tools(State(state.clone()), ViewPluginsAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "get_tool" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::tool_detail(
                State(state.clone()),
                Path(id),
                ViewPluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "install_tool" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::install_tool(
                State(state.clone()),
                Path(id),
                ManagePluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "get_pack" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::pack_detail(
                State(state.clone()),
                Path(id),
                ViewPluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "install_pack" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            plugins::install_pack(
                State(state.clone()),
                Path(id),
                ManagePluginsAuth(principal.clone()),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "list_local_catalog" => local::snapshot(State(state.clone()), AnyAuth(principal.clone()))
            .await
            .map(|Json(v)| McpToolResult::json(&v)),
        "upload_local_resource" => {
            let req: UploadLocalResourceRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            local::upload_resource(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "upsert_local_script" => {
            let req: UpsertLocalScriptRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            local::upsert_script(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "create_local_block_template" => {
            let req: CreateBlockTemplateRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            local::create_block_template(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "create_local_flow_template" => {
            let req: CreateFlowTemplateRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            local::create_flow_template(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "create_local_pack" => {
            let req: CreateLocalPackRequest = match parse_args(args) {
                Ok(req) => req,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            local::create_pack(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Json(req),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        "delete_local_item" => {
            let id = match req_str(&args, "id") {
                Ok(id) => id.to_owned(),
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            local::delete_item(
                State(state.clone()),
                ManagePluginsAuth(principal.clone()),
                Path(id),
            )
            .await
            .map(|Json(v)| McpToolResult::json(&v))
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|err| McpToolResult::error(format!("{err:?}"))))
}
