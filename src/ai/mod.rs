//! Assistant IA intégré au central : orchestration LLM multi-fournisseurs branchée sur les outils MCP (ADR 0015).

mod provider;
pub mod settings;

use crate::{
    api::{AdminAuth, AnyAuth, AppState},
    auth::{self, Principal},
    error::{AppError, AppResult},
    mcp::{self, protocol::McpTool},
    runtime_settings,
};
use axum::{Json, extract::State};
use flux_shared::{
    Permission,
    ai::{
        AiChatRequest, AiChatResponse, AiChatRole, AiClientStatus, AiSettingsView, AiTestRequest,
        AiTestResponse, AiToolCallTrace, MAX_CHAT_MESSAGES, UpdateAiSettingsRequest,
    },
    mcp::McpAuditOutcome,
};
use provider::{ConversationEntry, ModelTurnResponse, ProviderEndpoint};
use serde_json::{Value, json};
use std::time::Instant;

const MAX_TOOL_RESULT_CHARS: usize = 12_000;
const MAX_TOOL_CALLS_PER_ROUND: usize = 8;

fn is_tool_read_only(tool: &McpTool) -> bool {
    tool.annotations
        .as_ref()
        .is_some_and(|ann| ann.read_only_hint)
}

fn available_tools_for_assistant(principal: &Principal, allow_write: bool) -> Vec<McpTool> {
    mcp::allowed_tools(principal)
        .into_iter()
        .filter(|tool| allow_write || is_tool_read_only(tool))
        .collect()
}

fn build_system_prompt(
    principal: &Principal,
    allow_write: bool,
    context_hint: Option<&str>,
) -> String {
    let mode = if allow_write {
        "Lecture et actions d'écriture autorisées (dans les limites des droits du compte)."
    } else {
        "Mode lecture seule : aucune création, modification ou suppression n'est exécutée."
    };
    let mut prompt = format!(
        "Tu es l'assistant intégré de Flux de Fichiers (orchestrateur de traitement média). \
         Réponds en français, de façon claire, concise et factuelle à partir des données obtenues par tes outils.\n\
         Utilisateur actif : {} (rôle : {}).\n\
         Périmètre : {}\n\
         Consignes : privilégie `diagnose_job` pour analyser une tâche en échec, `simulate_flow` et `validate_flow` pour vérifier un flux avant sauvegarde, et `get_job_stats` / `list_workers` / `audit` pour l'état global. Ne réclame et n'invente jamais de secret.",
        principal.name,
        principal.role.as_str(),
        mode
    );
    if let Some(hint) = context_hint.map(str::trim).filter(|s| !s.is_empty()) {
        prompt.push_str("\nContexte courant de l'interface : ");
        prompt.push_str(hint);
    }
    prompt
}

fn truncate_tool_output(raw: String) -> String {
    let mut chars = raw.chars();
    let head: String = chars.by_ref().take(MAX_TOOL_RESULT_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}\n… [sortie tronquée]")
    } else {
        head
    }
}

/// Retourne l'état public léger de l'assistant IA pour la session active (`GET /api/v1/ai/status`).
pub async fn client_status(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
) -> AppResult<Json<AiClientStatus>> {
    auth::require(&principal, Permission::ViewOverview)?;
    let effective = settings::load_effective(&state.pool, &state.config).await;
    Ok(Json(effective.as_client_status()))
}

/// Retourne les réglages complets sans exposer la clé d'API (`GET /api/v1/ai/settings`).
pub async fn get_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<AiSettingsView>> {
    let effective = settings::load_effective(&state.pool, &state.config).await;
    Ok(Json(effective.as_view()))
}

/// Met à jour les réglages de l'assistant IA (`PUT /api/v1/ai/settings`).
pub async fn update_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<UpdateAiSettingsRequest>,
) -> AppResult<Json<AiSettingsView>> {
    let effective = settings::update(&state.pool, &state.config, request).await?;
    Ok(Json(effective.as_view()))
}

/// Teste la connectivité et liste les modèles disponibles du fournisseur (`POST /api/v1/ai/test`).
pub async fn test_connection(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<AiTestRequest>,
) -> AppResult<Json<AiTestResponse>> {
    let base_url = settings::validate_base_url(&request.base_url)?;
    let saved = settings::load_effective(&state.pool, &state.config).await;
    let api_key = match request.api_key {
        Some(ref raw) => {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        None => saved.api_key,
    };
    let runtime = runtime_settings::load_effective(&state.pool, &state.config).await;
    let endpoint = ProviderEndpoint {
        provider: request.provider,
        base_url,
        model: request.model.trim().to_owned(),
        api_key,
    };
    Ok(Json(
        provider::test_connection(&endpoint, runtime.integration_policy()).await,
    ))
}

/// Exécute un tour conversationnel avec résolution multi-tours des outils MCP (`POST /api/v1/ai/chat`).
pub async fn chat(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Json(request): Json<AiChatRequest>,
) -> AppResult<Json<AiChatResponse>> {
    auth::require(&principal, Permission::ViewOverview)?;
    if request.messages.is_empty()
        || request.messages.len() > MAX_CHAT_MESSAGES
        || request
            .messages
            .iter()
            .all(|msg| msg.content.trim().is_empty())
        || !matches!(
            request.messages.last().map(|m| m.role),
            Some(AiChatRole::User)
        )
    {
        return Err(AppError::Invalid);
    }

    let ai_cfg = settings::load_effective(&state.pool, &state.config).await;
    if !ai_cfg.enabled || !ai_cfg.as_client_status().available {
        return Err(AppError::Conflict);
    }

    let runtime = runtime_settings::load_effective(&state.pool, &state.config).await;
    let policy = runtime.integration_policy();
    let endpoint = ProviderEndpoint {
        provider: ai_cfg.provider,
        base_url: ai_cfg.base_url.clone(),
        model: ai_cfg.model.clone(),
        api_key: ai_cfg.api_key.clone(),
    };
    let tools = available_tools_for_assistant(&principal, ai_cfg.allow_write_tools);
    let system_prompt = build_system_prompt(
        &principal,
        ai_cfg.allow_write_tools,
        request.context_hint.as_deref(),
    );

    let mut entries = provider::initial_entries(&request.messages);
    let mut traces: Vec<AiToolCallTrace> = Vec::new();
    let mut rounds_used: u8 = 0;

    for _round in 0..ai_cfg.max_tool_rounds {
        rounds_used += 1;
        let turn = provider::complete_turn(&endpoint, &system_prompt, &entries, &tools, policy)
            .await
            .map_err(|err| {
                tracing::warn!(provider = %ai_cfg.provider, error = %err, "échec d'appel LLM");
                AppError::Internal
            })?;

        match turn {
            ModelTurnResponse::FinalText(reply) => {
                return Ok(Json(AiChatResponse {
                    reply,
                    model: ai_cfg.model,
                    provider: ai_cfg.provider,
                    tool_calls: traces,
                    rounds_used,
                }));
            }
            ModelTurnResponse::ToolCalls {
                assistant_content,
                mut calls,
            } => {
                calls.truncate(MAX_TOOL_CALLS_PER_ROUND);
                entries.push(ConversationEntry::AssistantToolCalls {
                    content: assistant_content,
                    calls: calls.clone(),
                });
                for call in calls {
                    let (content, is_error, trace) =
                        execute_assistant_tool(&state, &principal, &tools, &call).await;
                    traces.push(trace);
                    entries.push(ConversationEntry::ToolResult {
                        call_id: call.id,
                        tool_name: call.name,
                        content,
                        is_error,
                    });
                }
            }
        }
    }

    // Dernier tour sans outil pour obtenir une synthèse finale si le budget de tours est épuisé.
    rounds_used += 1;
    let final_reply = provider::complete_turn(&endpoint, &system_prompt, &entries, &[], policy)
        .await
        .map_err(|err| {
            tracing::warn!(provider = %ai_cfg.provider, error = %err, "échec de synthèse finale LLM");
            AppError::Internal
        })?;
    let reply = match final_reply {
        ModelTurnResponse::FinalText(text) => text,
        ModelTurnResponse::ToolCalls {
            assistant_content, ..
        } => assistant_content
            .unwrap_or_else(|| "Nombre maximal d'étapes d'outils atteint.".to_owned()),
    };

    Ok(Json(AiChatResponse {
        reply,
        model: ai_cfg.model,
        provider: ai_cfg.provider,
        tool_calls: traces,
        rounds_used,
    }))
}

async fn execute_assistant_tool(
    state: &AppState,
    principal: &Principal,
    exposed_tools: &[McpTool],
    call: &provider::ToolCallRequest,
) -> (String, bool, AiToolCallTrace) {
    let started = Instant::now();
    let exposed = exposed_tools.iter().find(|tool| tool.name == call.name);
    let read_only = exposed.is_some_and(is_tool_read_only);
    let args = if call.arguments.is_object() {
        call.arguments.clone()
    } else {
        json!({})
    };
    let target_summary = mcp::audit::summarize_target(&args);

    let missing_perm = mcp::tool_required_permissions(&call.name)
        .iter()
        .any(|&perm| !principal.allows(perm));

    if missing_perm || exposed.is_none() {
        let elapsed = started.elapsed().as_millis();
        let duration_ms = i64::try_from(elapsed).unwrap_or(0);
        let outcome_str = if exposed.is_none() && !missing_perm {
            "blocked_write_mode"
        } else {
            "forbidden"
        };
        let msg = if outcome_str == "blocked_write_mode" {
            "Outil refusé : l'assistant est configuré en mode lecture seule."
        } else {
            "Outil refusé : droits insuffisants pour votre compte."
        };
        mcp::audit::log_event(
            &state.pool,
            principal,
            &call.name,
            target_summary.as_deref(),
            McpAuditOutcome::Forbidden,
            Some(msg),
            duration_ms,
        )
        .await;
        return (
            msg.to_owned(),
            true,
            AiToolCallTrace {
                tool_name: call.name.clone(),
                read_only,
                outcome: outcome_str.to_owned(),
                duration_ms: u64::try_from(elapsed).unwrap_or(0),
            },
        );
    }

    let result = mcp::call_tool(&call.name, args, state, principal).await;
    let elapsed = started.elapsed().as_millis();
    let duration_ms = i64::try_from(elapsed).unwrap_or(0);
    match result {
        Some(res) => {
            let outcome = if res.is_error {
                McpAuditOutcome::Error
            } else {
                McpAuditOutcome::Ok
            };
            let err_summary = mcp::audit::summarize_tool_error(&res);
            mcp::audit::log_event(
                &state.pool,
                principal,
                &call.name,
                target_summary.as_deref(),
                outcome,
                err_summary.as_deref(),
                duration_ms,
            )
            .await;
            let text = res
                .content
                .into_iter()
                .map(|part| part.text)
                .collect::<Vec<_>>()
                .join("\n");
            (
                truncate_tool_output(text),
                res.is_error,
                AiToolCallTrace {
                    tool_name: call.name.clone(),
                    read_only,
                    outcome: outcome.as_str().to_owned(),
                    duration_ms: u64::try_from(elapsed).unwrap_or(0),
                },
            )
        }
        None => {
            mcp::audit::log_event(
                &state.pool,
                principal,
                &call.name,
                target_summary.as_deref(),
                McpAuditOutcome::Error,
                Some("outil inconnu"),
                duration_ms,
            )
            .await;
            (
                format!("Outil inconnu : {}", call.name),
                true,
                AiToolCallTrace {
                    tool_name: call.name.clone(),
                    read_only,
                    outcome: "error".to_owned(),
                    duration_ms: u64::try_from(elapsed).unwrap_or(0),
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::router, api_keys, config::Config, repository};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use flux_shared::{
        Role,
        ai::{AiChatMessage, AiProvider},
    };
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tower::ServiceExt;

    async fn test_app() -> anyhow::Result<(axum::Router, AppState, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        let config = Config {
            bind: ([127, 0, 0, 1], 3000).into(),
            database_url: "sqlite::memory:".into(),
            data_dir: dir.path().to_path_buf(),
            input_dir: dir.path().join("input"),
            config_dir: dir.path().to_path_buf(),
            tools_cache_dir: dir.path().join("tools"),
            plugins_dir: None,
            static_dir: "frontend/dist".into(),
            max_file_bytes: 2048,
            local_worker: false,
            web_origin: None,
            web_session_ttl_seconds: 28_800,
            tls: None,
            smtp: None,
        };
        let state = AppState::new(pool, Arc::new(config));
        let app = router(state.clone());
        Ok((app, state, dir))
    }

    async fn issue_key_header(
        pool: &sqlx::SqlitePool,
        name: &str,
        role: Role,
    ) -> anyhow::Result<String> {
        let issued = api_keys::issue(pool, name, role).await?;
        Ok(format!("Bearer {}", issued.raw))
    }

    #[tokio::test]
    async fn assistant_executes_read_tools_blocks_writes_when_readonly_and_audits()
    -> anyhow::Result<()> {
        let round_counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&round_counter);

        let mock_llm = axum::Router::new()
            .route(
                "/v1/models",
                axum::routing::get(|| async {
                    Json(json!({ "data": [{ "id": "mock-model" }] }))
                }),
            )
            .route(
                "/v1/chat/completions",
                post(move |Json(_body): Json<Value>| {
                    let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if count == 0 {
                            Json(json!({
                                "choices": [{
                                    "message": {
                                        "role": "assistant",
                                        "content": "",
                                        "tool_calls": [
                                            {
                                                "id": "call_1",
                                                "type": "function",
                                                "function": { "name": "list_libraries", "arguments": "{}" }
                                            },
                                            {
                                                "id": "call_2",
                                                "type": "function",
                                                "function": { "name": "delete_library", "arguments": "{\"id\":\"x\"}" }
                                            }
                                        ]
                                    }
                                }]
                            }))
                        } else {
                            Json(json!({
                                "choices": [{
                                    "message": {
                                        "role": "assistant",
                                        "content": "Aucune bibliothèque trouvée et suppression bloquée en mode lecture seule."
                                    }
                                }]
                            }))
                        }
                    }
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mock_addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_llm).await;
        });

        let (app, state, _dir) = test_app().await?;
        let auth_header = issue_key_header(&state.pool, "ai-test-admin", Role::Admin).await?;
        let worker_header = issue_key_header(&state.pool, "ai-test-worker", Role::Worker).await?;

        // 1. Enregistrement de la config IA avec allow_write_tools = false
        let save_req = Request::builder()
            .method("PUT")
            .uri("/api/v1/ai/settings")
            .header("authorization", &auth_header)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&UpdateAiSettingsRequest {
                enabled: true,
                provider: AiProvider::OpenAiCompatible,
                base_url: format!("http://{mock_addr}/v1"),
                model: "mock-model".into(),
                api_key: Some("sk-secret-value".into()),
                allow_write_tools: false,
                max_tool_rounds: 4,
            })?))?;
        let save_res = app.clone().oneshot(save_req).await?;
        assert_eq!(save_res.status(), StatusCode::OK);
        let view: AiSettingsView =
            serde_json::from_slice(&axum::body::to_bytes(save_res.into_body(), 16_384).await?)?;
        assert!(view.api_key_set);

        // 2. Test de connectivité réutilise la clé chiffrée stockée
        let test_req = Request::builder()
            .method("POST")
            .uri("/api/v1/ai/test")
            .header("authorization", &auth_header)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&AiTestRequest {
                provider: AiProvider::OpenAiCompatible,
                base_url: format!("http://{mock_addr}/v1"),
                model: "mock-model".into(),
                api_key: None,
            })?))?;
        let test_res = app.clone().oneshot(test_req).await?;
        assert_eq!(test_res.status(), StatusCode::OK);
        let test_body: AiTestResponse =
            serde_json::from_slice(&axum::body::to_bytes(test_res.into_body(), 16_384).await?)?;
        assert!(test_body.reachable);
        assert_eq!(test_body.available_models, vec!["mock-model"]);

        // 3. Appel chat : exécute list_libraries (ok) et bloque delete_library (blocked_write_mode)
        let chat_req = Request::builder()
            .method("POST")
            .uri("/api/v1/ai/chat")
            .header("authorization", &auth_header)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&AiChatRequest {
                messages: vec![AiChatMessage {
                    role: AiChatRole::User,
                    content: "Liste les bibliothèques".into(),
                }],
                context_hint: Some("Écran /libraries".into()),
            })?))?;
        let chat_res = app.clone().oneshot(chat_req).await?;
        assert_eq!(chat_res.status(), StatusCode::OK);
        let chat_body: AiChatResponse =
            serde_json::from_slice(&axum::body::to_bytes(chat_res.into_body(), 65_536).await?)?;
        assert_eq!(chat_body.rounds_used, 2);
        assert_eq!(chat_body.tool_calls.len(), 2);
        assert_eq!(chat_body.tool_calls[0].tool_name, "list_libraries");
        assert_eq!(chat_body.tool_calls[0].outcome, "ok");
        assert_eq!(chat_body.tool_calls[1].tool_name, "delete_library");
        assert_eq!(chat_body.tool_calls[1].outcome, "blocked_write_mode");

        // Vérifie la trace dans mcp_audit_log
        let audit_list = repository::list_mcp_audit(&state.pool, 10, None, None).await?;
        assert!(
            audit_list
                .entries
                .iter()
                .any(|e| e.tool_name == "list_libraries" && e.outcome == McpAuditOutcome::Ok)
        );
        assert!(audit_list.entries.iter().any(|e| e.tool_name == "delete_library" && e.outcome == McpAuditOutcome::Forbidden));

        // 4. Un worker ne peut pas appeler l'assistant IA
        let worker_req = Request::builder()
            .method("POST")
            .uri("/api/v1/ai/chat")
            .header("authorization", &worker_header)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&AiChatRequest {
                messages: vec![AiChatMessage {
                    role: AiChatRole::User,
                    content: "Bonjour".into(),
                }],
                context_hint: None,
            })?))?;
        let worker_res = app.oneshot(worker_req).await?;
        assert_eq!(worker_res.status(), StatusCode::FORBIDDEN);

        server_task.abort();
        Ok(())
    }
}
