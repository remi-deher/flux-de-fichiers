//! Serveur MCP natif (Streamable HTTP JSON-RPC 2.0 sur `/mcp`, ADR 0015).

mod protocol;
mod tools_flows;
mod tools_libraries;
mod tools_plugins;
mod tools_system;

use crate::{
    api::{
        AdminAuth, AppState, ViewFlowsAuth, ViewLibrariesAuth, ViewPluginsAuth, flows, libraries,
        plugins,
    },
    auth::{self, Principal},
    error::AppError,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use flux_shared::Permission;
use protocol::{
    ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, JsonRpcRequest, JsonRpcResponse,
    McpResource, McpTool, McpToolResult, PROTOCOL_VERSION, SERVER_NAME,
};
use serde::Deserialize;
use serde_json::{Value, json};

const OPENAPI_SPEC: &str = include_str!("../../docs/openapi.yaml");

fn all_tools() -> Vec<McpTool> {
    let mut tools = Vec::new();
    tools.extend(tools_flows::tools());
    tools.extend(tools_plugins::tools());
    tools.extend(tools_libraries::tools());
    tools.extend(tools_system::tools());
    tools
}

fn all_resources() -> Vec<McpResource> {
    vec![
        McpResource {
            uri: "flux://openapi.yaml",
            name: "Contrat OpenAPI",
            description: "Spécification OpenAPI 3.1.0 complète de l'API HTTP du central.",
            mime_type: "application/yaml",
        },
        McpResource {
            uri: "flux://flows",
            name: "Flux configurés",
            description: "Liste JSON des flux configurés (secrets masqués).",
            mime_type: "application/json",
        },
        McpResource {
            uri: "flux://libraries",
            name: "Bibliothèques",
            description: "Liste JSON des bibliothèques déclarées.",
            mime_type: "application/json",
        },
        McpResource {
            uri: "flux://plugins/catalog",
            name: "Catalogue des extensions",
            description: "Catalogue JSON des plugins, packs, flux modèles et scripts.",
            mime_type: "application/json",
        },
    ]
}

#[derive(Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Deserialize)]
struct ResourceReadParams {
    uri: String,
}

async fn call_tool(
    name: &str,
    args: Value,
    state: &AppState,
    principal: &Principal,
) -> Option<McpToolResult> {
    if let Some(res) = tools_flows::call(name, args.clone(), state, principal).await {
        return Some(res);
    }
    if let Some(res) = tools_plugins::call(name, args.clone(), state, principal).await {
        return Some(res);
    }
    if let Some(res) = tools_libraries::call(name, args.clone(), state, principal).await {
        return Some(res);
    }
    tools_system::call(name, args, state, principal).await
}

async fn read_resource(
    uri: &str,
    state: &AppState,
    principal: &Principal,
) -> Result<(String, &'static str), String> {
    match uri {
        "flux://openapi.yaml" => Ok((OPENAPI_SPEC.to_owned(), "application/yaml")),
        "flux://flows" => {
            let Json(list) =
                flows::list_flows(State(state.clone()), ViewFlowsAuth(principal.clone()))
                    .await
                    .map_err(|err| format!("{err:?}"))?;
            let text = serde_json::to_string_pretty(&list).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        "flux://libraries" => {
            let Json(list) =
                libraries::list(State(state.clone()), ViewLibrariesAuth(principal.clone()))
                    .await
                    .map_err(|err| format!("{err:?}"))?;
            let text = serde_json::to_string_pretty(&list).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        "flux://plugins/catalog" => {
            let Json(list) =
                plugins::catalog(State(state.clone()), ViewPluginsAuth(principal.clone()))
                    .await
                    .map_err(|err| format!("{err:?}"))?;
            let text = serde_json::to_string_pretty(&list).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        _ => Err(format!("ressource inconnue : {uri}")),
    }
}

/// Point d'entrée Streamable HTTP (`POST /mcp`).
pub async fn handle_post(
    State(state): State<AppState>,
    AdminAuth(principal): AdminAuth,
    Json(req): Json<JsonRpcRequest>,
) -> Result<Response, AppError> {
    auth::require(&principal, Permission::ManageSettings)?;
    if req.jsonrpc != "2.0" {
        let id = req.id.unwrap_or(Value::Null);
        return Ok(Json(JsonRpcResponse::err(
            id,
            ERR_INVALID_REQUEST,
            "jsonrpc \"2.0\" attendu",
        ))
        .into_response());
    }

    let Some(id) = req.id else {
        // Notification JSON-RPC (ex. notifications/initialized)
        return Ok(StatusCode::ACCEPTED.into_response());
    };

    let response = match req.method.as_str() {
        "initialize" => JsonRpcResponse::ok(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" => JsonRpcResponse::ok(id, json!({})),
        "tools/list" => JsonRpcResponse::ok(id, json!({ "tools": all_tools() })),
        "tools/call" => {
            let params_val = req.params.unwrap_or(Value::Null);
            match serde_json::from_value::<ToolCallParams>(params_val) {
                Ok(params) => {
                    let args = if params.arguments.is_null() {
                        json!({})
                    } else {
                        params.arguments
                    };
                    match call_tool(&params.name, args, &state, &principal).await {
                        Some(result) => {
                            tracing::info!(
                                principal_id = %principal.id,
                                role = %principal.role.as_str(),
                                tool = %params.name,
                                is_error = result.is_error,
                                "appel outil MCP"
                            );
                            let val = serde_json::to_value(result).unwrap_or(Value::Null);
                            JsonRpcResponse::ok(id, val)
                        }
                        None => JsonRpcResponse::err(
                            id,
                            ERR_METHOD_NOT_FOUND,
                            format!("outil inconnu : {}", params.name),
                        ),
                    }
                }
                Err(err) => JsonRpcResponse::err(
                    id,
                    ERR_INVALID_PARAMS,
                    format!("paramètres invalides : {err}"),
                ),
            }
        }
        "resources/list" => JsonRpcResponse::ok(id, json!({ "resources": all_resources() })),
        "resources/read" => {
            let params_val = req.params.unwrap_or(Value::Null);
            match serde_json::from_value::<ResourceReadParams>(params_val) {
                Ok(params) => match read_resource(&params.uri, &state, &principal).await {
                    Ok((text, mime_type)) => JsonRpcResponse::ok(
                        id,
                        json!({
                            "contents": [{
                                "uri": params.uri,
                                "mimeType": mime_type,
                                "text": text
                            }]
                        }),
                    ),
                    Err(msg) => JsonRpcResponse::err(id, ERR_INVALID_PARAMS, msg),
                },
                Err(err) => JsonRpcResponse::err(
                    id,
                    ERR_INVALID_PARAMS,
                    format!("paramètres invalides : {err}"),
                ),
            }
        }
        other => JsonRpcResponse::err(
            id,
            ERR_METHOD_NOT_FOUND,
            format!("méthode inconnue : {other}"),
        ),
    };

    Ok(Json(response).into_response())
}

/// `GET /mcp` : le serveur n'émet pas de flux SSE non sollicité.
pub async fn handle_get() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api, api_keys, config::Config};
    use axum::{
        body::Body,
        http::{Request, header},
    };
    use flux_shared::Role;
    use tower::ServiceExt;

    async fn test_app() -> anyhow::Result<(axum::Router, AppState, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let input_dir = dir.path().join("input");
        std::fs::create_dir_all(&input_dir)?;
        let config = Config {
            bind: "127.0.0.1:3000".parse()?,
            database_url: "sqlite::memory:".into(),
            data_dir: dir.path().to_path_buf(),
            input_dir,
            config_dir: dir.path().join("config"),
            tools_cache_dir: dir.path().join("tools-cache"),
            plugins_dir: None,
            static_dir: "frontend/dist".into(),
            max_file_bytes: 1024 * 1024,
            local_worker: false,
            web_origin: None,
            web_session_ttl_seconds: 28_800,
            tls: None,
            smtp: None,
        };
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&config.database_url)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        let state = AppState::new(pool, std::sync::Arc::new(config));
        Ok((api::router(state.clone()), state, dir))
    }

    async fn mcp_call(
        app: &axum::Router,
        token: &str,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))?;
        let resp = app.clone().oneshot(req).await?;
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await?;
        let parsed = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        };
        Ok((status, parsed))
    }

    #[tokio::test]
    async fn mcp_requires_mcp_or_admin_key_and_rejects_worker_or_viewer() -> anyhow::Result<()> {
        let (app, state, _dir) = test_app().await?;
        let mcp_key = api_keys::issue(&state.pool, "claude-mcp", Role::Mcp).await?;
        let worker_key = api_keys::issue(&state.pool, "worker-1", Role::Worker).await?;
        let viewer_key = api_keys::issue(&state.pool, "viewer-1", Role::Viewer).await?;

        let init = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let (st_worker, _) = mcp_call(&app, &worker_key.raw, init.clone()).await?;
        assert_eq!(st_worker, StatusCode::FORBIDDEN);

        let (st_viewer, _) = mcp_call(&app, &viewer_key.raw, init.clone()).await?;
        assert_eq!(st_viewer, StatusCode::FORBIDDEN);

        let (st_mcp, resp) = mcp_call(&app, &mcp_key.raw, init).await?;
        assert_eq!(st_mcp, StatusCode::OK);
        assert_eq!(resp["result"]["serverInfo"]["name"], SERVER_NAME);

        let (st_list, tools_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await?;
        assert_eq!(st_list, StatusCode::OK);
        let tools = tools_resp["result"]["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing tools array"))?;
        assert!(tools.iter().any(|t| t["name"] == "save_flow"));
        assert!(tools.iter().any(|t| t["name"] == "validate_flow"));
        assert!(tools.iter().any(|t| t["name"] == "restore_flow_version"));
        assert!(tools.iter().any(|t| t["name"] == "add_repository"));
        assert!(tools.iter().any(|t| t["name"] == "get_repository"));
        assert!(tools.iter().any(|t| t["name"] == "get_tool"));
        assert!(tools.iter().any(|t| t["name"] == "get_pack"));
        assert!(tools.iter().any(|t| t["name"] == "scan_library"));
        assert!(tools.iter().any(|t| t["name"] == "get_job_steps"));
        assert!(tools.iter().any(|t| t["name"] == "delete_job"));
        assert!(tools.iter().any(|t| t["name"] == "delete_input_file"));
        assert!(tools.iter().any(|t| t["name"] == "run_diagnostics"));
        assert!(tools.iter().any(|t| t["name"] == "get_instance_status"));
        assert!(tools.iter().any(|t| t["name"] == "delete_worker"));
        Ok(())
    }

    #[tokio::test]
    async fn mcp_can_create_flow_library_and_scan_input_file() -> anyhow::Result<()> {
        let (app, state, _dir) = test_app().await?;
        let mcp_key = api_keys::issue(&state.pool, "cursor-mcp", Role::Mcp).await?;
        std::fs::write(state.config.input_dir.join("film.mkv"), b"dummy-mkv-bytes")?;

        let (st, flow_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "save_flow",
                    "arguments": {
                        "id": "flow.mcp.test",
                        "name": "Flux MCP Test",
                        "isDefault": true,
                        "definition": {
                            "nodes": [
                                { "id": "n1", "type": "file-input", "name": "Entrée", "config": {} },
                                { "id": "n2", "type": "file-output", "name": "Sortie", "config": {} }
                            ],
                            "edges": [
                                { "from": "n1", "to": "n2" }
                            ]
                        }
                    }
                }
            }),
        )
        .await?;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(flow_resp["result"]["isError"], false);

        let (_, lib_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/call",
                "params": {
                    "name": "create_library",
                    "arguments": {
                        "name": "Films",
                        "flowId": "flow.mcp.test",
                        "workers": []
                    }
                }
            }),
        )
        .await?;
        assert_eq!(lib_resp["result"]["isError"], false);
        let lib_text = lib_resp["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing library json"))?;
        let lib_val: Value = serde_json::from_str(lib_text)?;
        let lib_id = lib_val["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing library id"))?;

        let (_, scan_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 12,
                "method": "tools/call",
                "params": {
                    "name": "scan_library",
                    "arguments": {
                        "id": lib_id
                    }
                }
            }),
        )
        .await?;
        assert_eq!(scan_resp["result"]["isError"], false);
        let scan_text = scan_resp["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing scan json"))?;
        let scan_val: Value = serde_json::from_str(scan_text)?;
        assert_eq!(scan_val["createdJobs"], 1);
        assert_eq!(scan_val["createdFiles"][0], "film.mkv");

        // Un second scan ignore le fichier déjà en file.
        let (_, scan_again) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 13,
                "method": "tools/call",
                "params": {
                    "name": "scan_library",
                    "arguments": {
                        "id": lib_id
                    }
                }
            }),
        )
        .await?;
        let scan_again_val: Value = serde_json::from_str(
            scan_again["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("{}"),
        )?;
        assert_eq!(scan_again_val["createdJobs"], 0);
        assert_eq!(scan_again_val["skippedExisting"], 1);
        Ok(())
    }
}
