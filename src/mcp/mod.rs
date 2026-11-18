//! Serveur MCP natif (Streamable HTTP JSON-RPC 2.0 sur `/mcp`, ADR 0015).

pub(crate) mod audit;
mod prompts;
pub(crate) mod protocol;
mod tools_flows;
mod tools_libraries;
mod tools_plugins;
mod tools_system;

use crate::{
    api::{
        AnyAuth, AppState, ManageApiKeysAuth, OPENAPI_SPEC, ViewFlowsAuth, ViewJobsAuth,
        ViewLibrariesAuth, ViewOverviewAuth, ViewPluginsAuth, ViewWorkersAuth, flows, jobs,
        libraries, plugins, system,
    },
    auth::{self, Principal},
    error::{AppError, AppResult},
    repository,
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use flux_shared::{
    Permission, Role,
    mcp::{McpAuditListResponse, McpAuditOutcome},
};
use protocol::{
    ERR_INVALID_PARAMS, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND, JsonRpcRequest, JsonRpcResponse,
    McpResource, McpTool, McpToolResult, PROTOCOL_VERSION, SERVER_NAME,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Instant;

pub(crate) fn tool_required_permissions(name: &str) -> &'static [Permission] {
    match name {
        "list_blocks" | "list_flows" | "get_flow" | "list_flow_versions" | "get_flow_version"
        | "validate_flow" | "simulate_flow" | "diff_flow_versions" => &[Permission::ViewFlows],
        "save_flow" | "delete_flow" | "restore_flow_version" => &[Permission::ManageFlows],
        "list_repositories" | "get_repository" | "list_catalog" | "get_plugin" | "list_tools"
        | "get_tool" | "get_pack" | "list_local_catalog" => &[Permission::ViewPlugins],
        "add_repository"
        | "sync_repository"
        | "toggle_repository"
        | "delete_repository"
        | "get_plugin_settings"
        | "save_plugin_settings"
        | "test_plugin_connection"
        | "install_tool"
        | "install_pack"
        | "upload_local_resource"
        | "upsert_local_script"
        | "create_local_block_template"
        | "create_local_flow_template"
        | "create_local_pack"
        | "delete_local_item" => &[Permission::ManagePlugins],
        "list_libraries" | "get_library" | "list_input_files" => &[Permission::ViewLibraries],
        "create_library" | "update_library" | "delete_library" | "delete_input_file"
        | "browse_filesystem" => &[Permission::ManageLibraries],
        "scan_library" => &[Permission::ManageLibraries, Permission::ManageJobs],
        "list_jobs" | "get_job" | "get_job_steps" | "get_job_stats" | "diagnose_job" => {
            &[Permission::ViewJobs]
        }
        "create_job"
        | "retry_job"
        | "cancel_job"
        | "update_job_priority"
        | "batch_jobs"
        | "delete_job"
        | "delete_job_output" => &[Permission::ManageJobs],
        "get_system_status" | "update_system_settings" | "run_diagnostics" => {
            &[Permission::ManageSettings]
        }
        "get_instance_status" | "get_system_hardware" => &[Permission::ViewOverview],
        "list_workers" | "list_enrollment_requests" => &[Permission::ViewWorkers],
        "update_worker_settings"
        | "delete_worker"
        | "set_enrollment_window"
        | "approve_enrollment_request"
        | "reject_enrollment_request" => &[Permission::ManageWorkers],
        _ => &[Permission::ManageSettings],
    }
}

pub(crate) fn allowed_tools(principal: &Principal) -> Vec<McpTool> {
    let mut tools = Vec::new();
    tools.extend(tools_flows::tools());
    tools.extend(tools_plugins::tools());
    tools.extend(tools_libraries::tools());
    tools.extend(tools_system::tools());
    tools
        .into_iter()
        .filter(|tool| {
            tool_required_permissions(tool.name)
                .iter()
                .all(|&perm| principal.allows(perm))
        })
        .collect()
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
        McpResource {
            uri: "flux://workers",
            name: "Workers enregistrés",
            description: "Liste JSON des workers, concurrence, plages horaires et télémétrie.",
            mime_type: "application/json",
        },
        McpResource {
            uri: "flux://jobs/recent",
            name: "Tâches récentes",
            description: "Les 50 dernières tâches traitées ou en file.",
            mime_type: "application/json",
        },
        McpResource {
            uri: "flux://jobs/stats",
            name: "Statistiques et économies disque",
            description: "Bilan volumétrique global et économies d'espace par bibliothèque.",
            mime_type: "application/json",
        },
        McpResource {
            uri: "flux://system/hardware",
            name: "Matériel du central",
            description: "Capacités CPU, mémoire, GPU et stockage du serveur central.",
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

#[derive(Deserialize)]
struct PromptGetParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

pub(crate) async fn call_tool(
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
        "flux://workers" => {
            let Json(list) =
                jobs::workers(State(state.clone()), ViewWorkersAuth(principal.clone()))
                    .await
                    .map_err(|err| format!("{err:?}"))?;
            let text = serde_json::to_string_pretty(&list).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        "flux://jobs/recent" => {
            auth::require(principal, Permission::ViewJobs).map_err(|err| format!("{err:?}"))?;
            let list = repository::list_jobs(&state.pool, None, 50)
                .await
                .map_err(|err| format!("{err:?}"))?;
            let text = serde_json::to_string_pretty(&list).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        "flux://jobs/stats" => {
            let Json(stats) = jobs::stats(State(state.clone()), ViewJobsAuth(principal.clone()))
                .await
                .map_err(|err| format!("{err:?}"))?;
            let text = serde_json::to_string_pretty(&stats).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        "flux://system/hardware" => {
            let Json(hw) =
                system::hardware(State(state.clone()), ViewOverviewAuth(principal.clone())).await;
            let text = serde_json::to_string_pretty(&hw).map_err(|err| err.to_string())?;
            Ok((text, "application/json"))
        }
        _ => Err(format!("ressource inconnue : {uri}")),
    }
}

/// Point d'entrée Streamable HTTP (`POST /mcp`).
pub async fn handle_post(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Json(req): Json<JsonRpcRequest>,
) -> Result<Response, AppError> {
    if !matches!(principal.role, Role::Admin | Role::Mcp) {
        return Err(AppError::Forbidden);
    }
    if req.jsonrpc != "2.0" {
        let id = req.id.unwrap_or(Value::Null);
        return Ok(Json(JsonRpcResponse::err(
            id,
            ERR_INVALID_REQUEST,
            "jsonrpc \"2.0\" attendu",
        ))
        .into_response());
    }
    if let Err(err) = audit::check_mcp_rate_limit(&principal.id) {
        let method_name = if req.method.is_empty() {
            "mcp"
        } else {
            req.method.as_str()
        };
        audit::log_event(
            &state.pool,
            &principal,
            method_name,
            None,
            McpAuditOutcome::RateLimited,
            Some("Débit maximal par clé dépassé (120 req/min)"),
            0,
        )
        .await;
        return Err(err);
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
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" => JsonRpcResponse::ok(id, json!({})),
        "tools/list" => JsonRpcResponse::ok(id, json!({ "tools": allowed_tools(&principal) })),
        "tools/call" => {
            let params_val = req.params.unwrap_or(Value::Null);
            match serde_json::from_value::<ToolCallParams>(params_val) {
                Ok(params) => {
                    let started = Instant::now();
                    let args = if params.arguments.is_null() {
                        json!({})
                    } else {
                        params.arguments
                    };
                    let target_summary = audit::summarize_target(&args);
                    let missing_perm = tool_required_permissions(&params.name)
                        .iter()
                        .any(|&perm| !principal.allows(perm));
                    let outcome = if missing_perm {
                        Some(McpToolResult::error(
                            "Forbidden: portée de clé insuffisante pour cet outil",
                        ))
                    } else {
                        call_tool(&params.name, args, &state, &principal).await
                    };
                    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(0);
                    match outcome {
                        Some(result) => {
                            let audit_outcome = if missing_perm {
                                McpAuditOutcome::Forbidden
                            } else if result.is_error {
                                McpAuditOutcome::Error
                            } else {
                                McpAuditOutcome::Ok
                            };
                            let err_summary = audit::summarize_tool_error(&result);
                            audit::log_event(
                                &state.pool,
                                &principal,
                                &params.name,
                                target_summary.as_deref(),
                                audit_outcome,
                                err_summary.as_deref(),
                                duration_ms,
                            )
                            .await;
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
                        None => {
                            audit::log_event(
                                &state.pool,
                                &principal,
                                &params.name,
                                target_summary.as_deref(),
                                McpAuditOutcome::Error,
                                Some("outil inconnu"),
                                duration_ms,
                            )
                            .await;
                            JsonRpcResponse::err(
                                id,
                                ERR_METHOD_NOT_FOUND,
                                format!("outil inconnu : {}", params.name),
                            )
                        }
                    }
                }
                Err(err) => {
                    let msg = format!("paramètres invalides : {err}");
                    audit::log_event(
                        &state.pool,
                        &principal,
                        "tools/call",
                        None,
                        McpAuditOutcome::Error,
                        audit::summarize_error_text(&msg).as_deref(),
                        0,
                    )
                    .await;
                    JsonRpcResponse::err(id, ERR_INVALID_PARAMS, msg)
                }
            }
        }
        "resources/list" => JsonRpcResponse::ok(id, json!({ "resources": all_resources() })),
        "resources/read" => {
            let params_val = req.params.unwrap_or(Value::Null);
            match serde_json::from_value::<ResourceReadParams>(params_val) {
                Ok(params) => {
                    let started = Instant::now();
                    let result = read_resource(&params.uri, &state, &principal).await;
                    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(0);
                    match result {
                        Ok((text, mime_type)) => {
                            audit::log_event(
                                &state.pool,
                                &principal,
                                "resources/read",
                                Some(&params.uri),
                                McpAuditOutcome::Ok,
                                None,
                                duration_ms,
                            )
                            .await;
                            JsonRpcResponse::ok(
                                id,
                                json!({
                                    "contents": [{
                                        "uri": params.uri,
                                        "mimeType": mime_type,
                                        "text": text
                                    }]
                                }),
                            )
                        }
                        Err(msg) => {
                            audit::log_event(
                                &state.pool,
                                &principal,
                                "resources/read",
                                Some(&params.uri),
                                McpAuditOutcome::Error,
                                audit::summarize_error_text(&msg).as_deref(),
                                duration_ms,
                            )
                            .await;
                            JsonRpcResponse::err(id, ERR_INVALID_PARAMS, msg)
                        }
                    }
                }
                Err(err) => JsonRpcResponse::err(
                    id,
                    ERR_INVALID_PARAMS,
                    format!("paramètres invalides : {err}"),
                ),
            }
        }
        "prompts/list" => JsonRpcResponse::ok(id, json!({ "prompts": prompts::list() })),
        "prompts/get" => {
            let params_val = req.params.unwrap_or(Value::Null);
            match serde_json::from_value::<PromptGetParams>(params_val) {
                Ok(params) => match prompts::get(&params.name, &params.arguments) {
                    Ok((description, messages)) => JsonRpcResponse::ok(
                        id,
                        json!({
                            "description": description,
                            "messages": messages,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub key_id: Option<String>,
    pub outcome: Option<String>,
}

/// Historique des appels MCP (`GET /api/v1/mcp/audit`), réservé aux administrateurs.
pub async fn list_audit(
    State(state): State<AppState>,
    _auth: ManageApiKeysAuth,
    Query(query): Query<AuditQuery>,
) -> AppResult<Json<McpAuditListResponse>> {
    let outcome = match query.outcome.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(McpAuditOutcome::parse(raw).ok_or(AppError::Invalid)?),
    };
    let limit = query.limit.unwrap_or(repository::DEFAULT_AUDIT_LIMIT);
    let response =
        repository::list_mcp_audit(&state.pool, limit, query.key_id.as_deref(), outcome).await?;
    Ok(Json(response))
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

        // Une clé MCP avec portée restreinte aux flux ne voit que les outils de flux et ne peut pas créer de bibliothèque.
        let scoped_key = api_keys::issue_with_scopes(
            &state.pool,
            "flows-only",
            Role::Mcp,
            Some(&[Permission::ViewFlows, Permission::ManageFlows]),
        )
        .await?;
        let (st_scoped_list, scoped_list) = mcp_call(
            &app,
            &scoped_key.raw,
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
        )
        .await?;
        assert_eq!(st_scoped_list, StatusCode::OK);
        let scoped_tools = scoped_list["result"]["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing scoped tools array"))?;
        assert!(scoped_tools.iter().any(|t| t["name"] == "save_flow"));
        assert!(!scoped_tools.iter().any(|t| t["name"] == "create_library"));

        let (st_forbidden, forbidden_call) = mcp_call(
            &app,
            &scoped_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "create_library",
                    "arguments": { "name": "Interdit", "workers": [] }
                }
            }),
        )
        .await?;
        assert_eq!(st_forbidden, StatusCode::OK);
        assert_eq!(forbidden_call["result"]["isError"], true);
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

    #[tokio::test]
    async fn mcp_records_audit_history_masks_secrets_and_enforces_rate_limit() -> anyhow::Result<()>
    {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let (app, state, _dir) = test_app().await?;
        let admin_key = api_keys::issue(&state.pool, "admin-audit", Role::Admin).await?;
        let mcp_key = api_keys::issue_with_scopes(
            &state.pool,
            "scoped-audit-mcp",
            Role::Mcp,
            Some(&[Permission::ViewFlows, Permission::ManagePlugins]),
        )
        .await?;

        // 1. Succès (`list_flows`) et erreur avec secret potentiel (`save_plugin_settings` sur plugin inconnu)
        let (st_ok, _) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 100,
                "method": "tools/call",
                "params": { "name": "list_flows", "arguments": {} }
            }),
        )
        .await?;
        assert_eq!(st_ok, StatusCode::OK);

        let (st_err, _) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 101,
                "method": "tools/call",
                "params": {
                    "name": "save_plugin_settings",
                    "arguments": {
                        "id": "plugin.radarr",
                        "settings": { "apiKey": "radarr-top-secret-key" }
                    }
                }
            }),
        )
        .await?;
        assert_eq!(st_err, StatusCode::OK);

        // 2. Appel interdit (`scan_library` non inclus dans la portée)
        let (st_forbidden, _) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 102,
                "method": "tools/call",
                "params": { "name": "scan_library", "arguments": { "id": "lib-1" } }
            }),
        )
        .await?;
        assert_eq!(st_forbidden, StatusCode::OK);

        // 3. Une clé MCP ne peut pas consulter l'historique d'audit ; seul l'admin le peut.
        let forbidden_audit = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/mcp/audit")
                    .header("authorization", format!("Bearer {}", mcp_key.raw))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(forbidden_audit.status(), StatusCode::FORBIDDEN);

        let admin_audit = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/mcp/audit?limit=10")
                    .header("authorization", format!("Bearer {}", admin_key.raw))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(admin_audit.status(), StatusCode::OK);
        let body = axum::body::to_bytes(admin_audit.into_body(), usize::MAX).await?;
        let audit_json: Value = serde_json::from_slice(&body)?;
        assert_eq!(audit_json["total"], 3);
        let text_dump = audit_json.to_string();
        assert!(!text_dump.contains("radarr-top-secret-key"));
        assert!(text_dump.contains("plugin.radarr"));

        let filtered = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/mcp/audit?outcome=forbidden")
                    .header("authorization", format!("Bearer {}", admin_key.raw))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(filtered.status(), StatusCode::OK);
        let filtered_json: Value =
            serde_json::from_slice(&axum::body::to_bytes(filtered.into_body(), usize::MAX).await?)?;
        assert_eq!(filtered_json["total"], 1);
        assert_eq!(filtered_json["entries"][0]["toolName"], "scan_library");
        assert_eq!(filtered_json["entries"][0]["outcome"], "forbidden");

        // 4. Dépassement du plafond de requêtes MCP -> HTTP 429 + événement `rate_limited`.
        for seq in 4..=audit::MCP_RATE_LIMIT_MAX {
            let (st_ping, _) = mcp_call(
                &app,
                &mcp_key.raw,
                json!({ "jsonrpc": "2.0", "id": seq, "method": "ping" }),
            )
            .await?;
            assert_eq!(st_ping, StatusCode::OK);
        }
        let (st_limited, _) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({ "jsonrpc": "2.0", "id": 999, "method": "ping" }),
        )
        .await?;
        assert_eq!(st_limited, StatusCode::TOO_MANY_REQUESTS);

        let limited_audit = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/mcp/audit?outcome=rate_limited")
                    .header("authorization", format!("Bearer {}", admin_key.raw))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(limited_audit.status(), StatusCode::OK);
        let limited_json: Value = serde_json::from_slice(
            &axum::body::to_bytes(limited_audit.into_body(), usize::MAX).await?,
        )?;
        assert_eq!(limited_json["total"], 1);
        assert_eq!(limited_json["entries"][0]["outcome"], "rate_limited");
        Ok(())
    }

    #[tokio::test]
    async fn mcp_exposes_annotations_prompts_new_resources_simulation_diff_and_diagnostics()
    -> anyhow::Result<()> {
        let (app, state, _dir) = test_app().await?;
        let mcp_key = api_keys::issue(&state.pool, "smart-mcp", Role::Mcp).await?;

        // 1. Capabilities + annotations sur tools/list + prompts/list + prompts/get
        let (st_init, init_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
        )
        .await?;
        assert_eq!(st_init, StatusCode::OK);
        assert!(init_resp["result"]["capabilities"]["prompts"].is_object());

        let (_, tools_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await?;
        let tools = tools_resp["result"]["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("tools array expected"))?;
        let sim_tool = tools
            .iter()
            .find(|t| t["name"] == "simulate_flow")
            .ok_or_else(|| anyhow::anyhow!("simulate_flow missing"))?;
        assert_eq!(sim_tool["annotations"]["readOnlyHint"], true);
        let del_tool = tools
            .iter()
            .find(|t| t["name"] == "delete_flow")
            .ok_or_else(|| anyhow::anyhow!("delete_flow missing"))?;
        assert_eq!(del_tool["annotations"]["destructiveHint"], true);

        let (_, prompts_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({ "jsonrpc": "2.0", "id": 3, "method": "prompts/list" }),
        )
        .await?;
        assert_eq!(
            prompts_resp["result"]["prompts"]
                .as_array()
                .map_or(0, Vec::len),
            3
        );
        let (_, prompt_get) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "prompts/get",
                "params": { "name": "design_flow", "arguments": { "goal": "AV1 VAAPI" } }
            }),
        )
        .await?;
        assert!(
            prompt_get["result"]["messages"][0]["content"]["text"]
                .as_str()
                .unwrap_or("")
                .contains("simulate_flow")
        );

        // 2. Sauvegarde de deux versions d'un flux + simulation + diff
        let v1_def = json!({
            "nodes": [
                { "id": "n1", "type": "file-input", "name": "Entrée", "config": {} },
                { "id": "n2", "type": "file-output", "name": "Sortie", "config": {} }
            ],
            "connections": [
                { "from": "n1", "to": "n2" }
            ]
        });
        let (_, _) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "save_flow",
                    "arguments": {
                        "id": "flow.ai.sim",
                        "name": "Flux IA",
                        "isDefault": true,
                        "definition": v1_def
                    }
                }
            }),
        )
        .await?;
        let v2_def = json!({
            "nodes": [
                { "id": "n1", "type": "file-input", "name": "Entrée", "config": {} },
                { "id": "n2", "type": "file-output", "name": "Sortie", "config": {} },
                {
                    "id": "n3",
                    "type": "webhook",
                    "name": "Webhook",
                    "config": { "custom_headers": { "Authorization": "Bearer should-never-leak" } }
                }
            ],
            "connections": [
                { "from": "n1", "to": "n3" },
                { "from": "n3", "to": "n2" }
            ]
        });
        let (_, _) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "save_flow",
                    "arguments": {
                        "id": "flow.ai.sim",
                        "name": "Flux IA",
                        "isDefault": true,
                        "definition": v2_def
                    }
                }
            }),
        )
        .await?;

        let (_, sim_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "simulate_flow",
                    "arguments": { "flowId": "flow.ai.sim", "preset": "structured_tracks" }
                }
            }),
        )
        .await?;
        assert_eq!(sim_resp["result"]["isError"], false);
        let sim_val: Value = serde_json::from_str(
            sim_resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("{}"),
        )?;
        assert_eq!(sim_val["status"], "complete");
        assert_eq!(sim_val["steps"].as_array().map_or(0, Vec::len), 3);

        let (_, diff_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": "diff_flow_versions",
                    "arguments": { "id": "flow.ai.sim" }
                }
            }),
        )
        .await?;
        let diff_val: Value = serde_json::from_str(
            diff_resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("{}"),
        )?;
        assert_eq!(diff_val["fromVersion"], 1);
        assert_eq!(diff_val["toVersion"], 2);
        assert_eq!(diff_val["diff"]["addedNodes"][0]["id"], "n3");

        // 3. Création d'une tâche + diagnose_job (vérifie masquage du secret webhook) + get_job_stats + ressource stats
        let job =
            repository::insert_job(&state.pool, "movie.mkv", None, &"a".repeat(64), 1_000_000)
                .await?;
        let (_, diag_resp) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": { "name": "diagnose_job", "arguments": { "id": job.id } }
            }),
        )
        .await?;
        assert_eq!(diag_resp["result"]["isError"], false);
        let diag_text = diag_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("");
        assert!(!diag_text.contains("should-never-leak"));

        let (_, res_stats) = mcp_call(
            &app,
            &mcp_key.raw,
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "resources/read",
                "params": { "uri": "flux://jobs/stats" }
            }),
        )
        .await?;
        let stats_val: Value = serde_json::from_str(
            res_stats["result"]["contents"][0]["text"]
                .as_str()
                .unwrap_or("{}"),
        )?;
        assert_eq!(stats_val["totalJobs"], 1);
        Ok(())
    }
}
