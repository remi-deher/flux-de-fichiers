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
        McpTool::read_only(
            "list_repositories",
            "Liste les dépôts de plugins configurés.",
            empty_schema(),
        ),
        McpTool::read_only(
            "get_repository",
            "Lit le détail d'un dépôt de plugins et ses entrées synchronisées.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "add_repository",
            "Ajoute un dépôt de plugins distant (URL vers index.json) et le synchronise.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "indexUrl": { "type": "string" }
                }),
                &["name", "indexUrl"],
            ),
            false,
        ),
        McpTool::write(
            "sync_repository",
            "Synchronise immédiatement un dépôt de plugins.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            true,
        ),
        McpTool::write(
            "toggle_repository",
            "Active ou désactive un dépôt de plugins.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            false,
        ),
        McpTool::destructive(
            "delete_repository",
            "Supprime un dépôt de plugins non officiel.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "list_catalog",
            "Liste les plugins, packs, flux modèles et scripts du catalogue synchronisé.",
            empty_schema(),
        ),
        McpTool::read_only(
            "get_plugin",
            "Lit le détail d'un plugin (blocs exposés, dépendances, paramètres).",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::read_only(
            "get_plugin_settings",
            "Lit les réglages d'un plugin (les secrets sont masqués).",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "save_plugin_settings",
            "Enregistre les réglages d'un plugin (les secrets sont chiffrés en base).",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "settings": { "type": "object" }
                }),
                &["id", "settings"],
            ),
            true,
        ),
        McpTool::read_only(
            "test_plugin_connection",
            "Teste la connectivité réseau et les identifiants d'un plugin API (Sonarr, Radarr, Jellyfin, Plex, Emby, etc.).",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "settings": { "type": "object" }
                }),
                &["id"],
            ),
        ),
        McpTool::read_only(
            "list_tools",
            "Liste les applications/outils distribués et leur état d'installation.",
            empty_schema(),
        ),
        McpTool::read_only(
            "get_tool",
            "Lit le détail d'une application distribuée (artefacts, SHA-256, compatibilité).",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "install_tool",
            "Télécharge, vérifie (SHA-256) et installe une application distribuée dans le cache.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            true,
        ),
        McpTool::read_only(
            "get_pack",
            "Lit le détail d'un pack (composants et plan d'installation).",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
        McpTool::write(
            "install_pack",
            "Installe un pack du catalogue et ses outils requis.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
            true,
        ),
        McpTool::read_only(
            "list_local_catalog",
            "Liste le catalogue local (scripts, ressources, modèles de blocs/flux, packs).",
            empty_schema(),
        ),
        McpTool::write(
            "upload_local_resource",
            "Dépose une ressource locale (contenu encodé en base64).",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "filename": { "type": "string" },
                    "contentBase64": { "type": "string" }
                }),
                &["name", "filename", "contentBase64"],
            ),
            true,
        ),
        McpTool::write(
            "upsert_local_script",
            "Crée ou met à jour un script local exécutable dans les flux.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "interpreter": { "type": "string" },
                    "content": { "type": "string" }
                }),
                &["name", "interpreter", "content"],
            ),
            true,
        ),
        McpTool::write(
            "create_local_block_template",
            "Enregistre un modèle de bloc local.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "block": { "type": "object" }
                }),
                &["name", "block"],
            ),
            false,
        ),
        McpTool::write(
            "create_local_flow_template",
            "Enregistre un modèle de flux local.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "definition": { "type": "object" }
                }),
                &["name", "definition"],
            ),
            false,
        ),
        McpTool::write(
            "create_local_pack",
            "Crée un pack local regroupant des scripts, ressources et modèles.",
            obj_schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "components": { "type": "array" }
                }),
                &["name", "components"],
            ),
            false,
        ),
        McpTool::destructive(
            "delete_local_item",
            "Supprime un élément du catalogue local par son identifiant.",
            obj_schema(json!({ "id": { "type": "string" } }), &["id"]),
        ),
    ]
}

#[derive(Deserialize)]
struct SaveSettingsArgs {
    id: String,
    settings: Value,
}

#[derive(Deserialize)]
struct TestConnectionArgs {
    id: String,
    #[serde(default)]
    settings: Option<Value>,
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
        "test_plugin_connection" => {
            let parsed: TestConnectionArgs = match parse_args(args) {
                Ok(parsed) => parsed,
                Err(msg) => return Some(McpToolResult::error(msg)),
            };
            let body = match parsed.settings {
                Some(val) => match serde_json::to_vec(&val) {
                    Ok(bytes) => axum::body::Bytes::from(bytes),
                    Err(err) => return Some(McpToolResult::error(err.to_string())),
                },
                None => axum::body::Bytes::new(),
            };
            plugins::test_connection(
                State(state.clone()),
                Path(parsed.id),
                ManagePluginsAuth(principal.clone()),
                body,
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
