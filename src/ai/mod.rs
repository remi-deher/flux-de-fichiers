//! Assistant IA intégré au central : orchestration LLM multi-fournisseurs branchée sur les outils MCP (ADR 0015).

pub mod history;
pub mod oauth;
mod preview;
mod provider;
pub mod run;
pub mod settings;

use crate::net_policy::OutboundPolicy;
use crate::secrets::SecretBox;
use crate::{
    api::{AdminAuth, AnyAuth, AppState},
    auth::{self, Principal},
    error::{AppError, AppResult},
    mcp::{self, protocol::McpTool},
    runtime_settings,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use flux_shared::{
    Permission,
    ai::{
        AiActionDecision, AiChatRequest, AiChatResponse, AiChatRole, AiClientStatus, AiContext,
        AiConversation, AiConversationList, AiOauthDeviceCode, AiOauthPollResponse, AiProvider,
        AiRunEvent, AiRunStarted, AiSettingsView, AiTestRequest, AiTestResponse, AiToolCallTrace,
        MAX_CHAT_MESSAGES, UpdateAiSettingsRequest,
    },
    mcp::McpAuditOutcome,
};
use futures_util::Stream;
use provider::{ConversationEntry, ModelTurnResponse, ProviderEndpoint};
use run::Run;
use serde_json::json;
use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_TOOL_RESULT_CHARS: usize = 12_000;
const MAX_TOOL_CALLS_PER_ROUND: usize = 8;
const SSE_KEEP_ALIVE: Duration = Duration::from_secs(15);
const CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(600);
const DECLINED_OUTCOME: &str = "declined";
const MAX_CONTEXT_FIELD_CHARS: usize = 128;

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
    context: Option<&AiContext>,
) -> String {
    let mode = if allow_write {
        "Lecture et actions d'écriture autorisées dans les limites des droits du compte ; chaque écriture est d'abord soumise à la confirmation de l'utilisateur, qui peut la refuser."
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
    if let Some(context) = context {
        prompt.push_str(&describe_context(context));
    }
    prompt
}

/// Garde les caractères sûrs d'un champ de contexte : il finit dans l'invite système.
fn clean_context_field(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control() && *c != '`')
        .take(MAX_CONTEXT_FIELD_CHARS)
        .collect()
}

/// Phrase d'invite décrivant l'écran et l'objet ouverts (la lecture passe par les outils).
fn describe_context(context: &AiContext) -> String {
    let mut text = format!(
        "\nÉcran ouvert : `{}`.",
        clean_context_field(&context.screen)
    );
    if let Some(object) = &context.object {
        text.push_str(&format!(
            " Objet affiché : {} `{}` ; lis-le avec tes outils avant de répondre sur « ce flux », « cette tâche » ou équivalent.",
            object.kind.label(),
            clean_context_field(&object.id)
        ));
    }
    text
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

/// Construit le point d'accès du fournisseur ; pour une connexion OAuth, le jeton d'accès est
/// rafraîchi au besoin (`Conflict` si la connexion est absente ou à refaire).
async fn resolve_endpoint(
    state: &AppState,
    provider: AiProvider,
    base_url: String,
    model: String,
    api_key: Option<String>,
    policy: OutboundPolicy,
) -> AppResult<ProviderEndpoint> {
    if !provider.uses_oauth() {
        return Ok(ProviderEndpoint {
            provider,
            base_url,
            model,
            api_key,
            account_id: None,
        });
    }
    let tokens =
        oauth::fresh_tokens(&state.pool, &state.config, oauth::OAUTH_ISSUER, policy).await?;
    Ok(ProviderEndpoint {
        provider,
        base_url,
        model,
        api_key: Some(tokens.access_token),
        account_id: tokens.account_id,
    })
}

/// Premier modèle proposé au compte, quand aucun modèle n'est choisi (comme Codex CLI).
async fn first_available_model(
    endpoint: &ProviderEndpoint,
    policy: OutboundPolicy,
) -> AppResult<String> {
    let models = provider::list_chatgpt_models(endpoint, policy)
        .await
        .map_err(|err| {
            tracing::warn!(provider = %endpoint.provider, error = %err, "liste des modèles indisponible");
            AppError::Internal
        })?;
    models
        .into_iter()
        .next()
        .map(|model| model.id)
        .ok_or_else(|| {
            tracing::warn!(provider = %endpoint.provider, "aucun modèle proposé à ce compte");
            AppError::Internal
        })
}

async fn integration_policy(state: &AppState) -> OutboundPolicy {
    runtime_settings::load_effective(&state.pool, &state.config)
        .await
        .integration_policy()
}

/// Démarre la connexion OAuth et renvoie le code à valider (`POST /api/v1/ai/oauth/start`).
pub async fn oauth_start(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<AiOauthDeviceCode>> {
    let policy = integration_policy(&state).await;
    Ok(Json(
        oauth::start(&state.pool, oauth::OAUTH_ISSUER, policy).await?,
    ))
}

/// Interroge la connexion OAuth en cours (`POST /api/v1/ai/oauth/poll`).
pub async fn oauth_poll(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<AiOauthPollResponse>> {
    let policy = integration_policy(&state).await;
    Ok(Json(
        oauth::poll(&state.pool, &state.config, oauth::OAUTH_ISSUER, policy).await?,
    ))
}

/// Oublie la connexion OAuth enregistrée (`DELETE /api/v1/ai/oauth`).
pub async fn oauth_disconnect(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<StatusCode> {
    oauth::disconnect(&state.pool, &state.config).await?;
    Ok(StatusCode::NO_CONTENT)
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
    let policy = integration_policy(&state).await;
    let endpoint = match resolve_endpoint(
        &state,
        request.provider,
        base_url,
        request.model.trim().to_owned(),
        api_key,
        policy,
    )
    .await
    {
        Ok(endpoint) => endpoint,
        Err(AppError::Conflict) => {
            return Ok(Json(AiTestResponse {
                reachable: false,
                latency_ms: 0,
                available_models: Vec::new(),
                message: "Aucun compte connecté : utilisez « Se connecter avec ChatGPT ».".into(),
            }));
        }
        Err(err) => return Err(err),
    };
    Ok(Json(provider::test_connection(&endpoint, policy).await))
}

/// Rejette une conversation vide, trop longue ou qui ne se termine pas par une question.
fn validate_chat_request(request: &AiChatRequest) -> AppResult<()> {
    let invalid = request.messages.is_empty()
        || request.messages.len() > MAX_CHAT_MESSAGES
        || request
            .messages
            .iter()
            .all(|msg| msg.content.trim().is_empty())
        || !matches!(
            request.messages.last().map(|m| m.role),
            Some(AiChatRole::User)
        );
    if invalid {
        Err(AppError::Invalid)
    } else {
        Ok(())
    }
}

/// Message montré à l'utilisateur quand une conversation échoue.
fn failure_message(error: &AppError) -> &'static str {
    match error {
        AppError::Conflict => {
            "L'assistant IA n'est pas encore activé ou configuré (ou le compte ChatGPT doit être reconnecté)."
        }
        AppError::Internal => {
            "Le fournisseur IA a échoué : consultez Activité > Journal pour le détail."
        }
        _ => "Requête refusée.",
    }
}

/// Exécute un tour conversationnel avec résolution multi-tours des outils MCP (`POST /api/v1/ai/chat`).
pub async fn chat(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Json(request): Json<AiChatRequest>,
) -> AppResult<Json<AiChatResponse>> {
    auth::require(&principal, Permission::ViewOverview)?;
    validate_chat_request(&request)?;
    converse(&state, &principal, request, None).await.map(Json)
}

/// Boucle de conversation partagée par `/ai/chat` et les runs.
///
/// Un `run` reçoit l'avancement des outils et peut demander confirmation avant une écriture ;
/// sans run (`/ai/chat`), personne ne peut confirmer : seuls les outils en lecture sont proposés.
async fn converse(
    state: &AppState,
    principal: &Principal,
    request: AiChatRequest,
    run: Option<&Run>,
) -> AppResult<AiChatResponse> {
    let ai_cfg = settings::load_effective(&state.pool, &state.config).await;
    if !ai_cfg.enabled || !ai_cfg.as_client_status().available {
        return Err(AppError::Conflict);
    }

    let policy = integration_policy(state).await;
    let mut endpoint = resolve_endpoint(
        state,
        ai_cfg.provider,
        ai_cfg.base_url.clone(),
        ai_cfg.model.clone(),
        ai_cfg.api_key.clone(),
        policy,
    )
    .await?;
    if ai_cfg.provider.has_automatic_model() && endpoint.model.trim().is_empty() {
        endpoint.model = first_available_model(&endpoint, policy).await?;
    }
    let writes_allowed = ai_cfg.allow_write_tools && run.is_some();
    let tools = available_tools_for_assistant(principal, writes_allowed);
    let system_prompt = build_system_prompt(
        principal,
        writes_allowed,
        request.context_hint.as_deref(),
        request.context.as_ref(),
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
                return Ok(AiChatResponse {
                    reply,
                    model: endpoint.model.clone(),
                    provider: ai_cfg.provider,
                    tool_calls: traces,
                    rounds_used,
                    conversation_id: None,
                });
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
                    publish(
                        run,
                        AiRunEvent::ToolStarted {
                            tool_name: call.name.clone(),
                        },
                    );
                    let (content, is_error, trace) =
                        execute_with_confirmation(state, principal, &tools, &call, run).await;
                    publish(
                        run,
                        AiRunEvent::ToolFinished {
                            trace: trace.clone(),
                        },
                    );
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

    Ok(AiChatResponse {
        reply,
        model: endpoint.model.clone(),
        provider: ai_cfg.provider,
        tool_calls: traces,
        rounds_used,
        conversation_id: None,
    })
}

fn publish(run: Option<&Run>, event: AiRunEvent) {
    if let Some(run) = run {
        run.publish(event);
    }
}

/// Exécute un outil ; une écriture attend d'abord l'autorisation de l'utilisateur (aperçu sans secrets).
async fn execute_with_confirmation(
    state: &AppState,
    principal: &Principal,
    tools: &[McpTool],
    call: &provider::ToolCallRequest,
    run: Option<&Run>,
) -> (String, bool, AiToolCallTrace) {
    let needs_confirmation = tools
        .iter()
        .find(|tool| tool.name == call.name)
        .is_some_and(|tool| !is_tool_read_only(tool));
    let (Some(run), true) = (run, needs_confirmation) else {
        return execute_assistant_tool(state, principal, tools, call).await;
    };
    let action_id = uuid::Uuid::new_v4().to_string();
    run.publish(AiRunEvent::Confirmation {
        action_id: action_id.clone(),
        tool_name: call.name.clone(),
        summary: preview::action_preview(&call.arguments),
    });
    let started = Instant::now();
    let approved = run.await_decision(&action_id, CONFIRMATION_TIMEOUT).await;
    run.publish(AiRunEvent::ActionResolved {
        action_id,
        approved,
    });
    if approved {
        return execute_assistant_tool(state, principal, tools, call).await;
    }
    let elapsed = started.elapsed().as_millis();
    let message = "Action refusée par l'utilisateur (ou sans réponse dans le délai).";
    mcp::audit::log_event(
        &state.pool,
        principal,
        &call.name,
        mcp::audit::summarize_target(&call.arguments).as_deref(),
        McpAuditOutcome::Forbidden,
        Some(message),
        i64::try_from(elapsed).unwrap_or(0),
    )
    .await;
    (
        message.to_owned(),
        true,
        AiToolCallTrace {
            tool_name: call.name.clone(),
            read_only: false,
            outcome: DECLINED_OUTCOME.to_owned(),
            duration_ms: u64::try_from(elapsed).unwrap_or(0),
        },
    )
}

/// Enregistre l'échange dans l'historique du compte ; un échec n'empêche pas la réponse.
async fn save_exchange(
    state: &AppState,
    principal: &Principal,
    conversation_id: Option<&str>,
    question: &str,
    reply: &str,
) -> Option<String> {
    let saved = async {
        let secrets = SecretBox::open(&state.config.config_dir)?;
        history::append_exchange(
            &state.pool,
            &secrets,
            &principal.id,
            conversation_id,
            question,
            reply,
        )
        .await
    }
    .await;
    saved
        .inspect_err(|error| tracing::warn!(%error, "historique de l'assistant non enregistré"))
        .ok()
}

/// Historique du compte (`GET /api/v1/ai/conversations`).
pub async fn list_conversations(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
) -> AppResult<Json<AiConversationList>> {
    auth::require(&principal, Permission::ViewOverview)?;
    let secrets = SecretBox::open(&state.config.config_dir)?;
    history::list(&state.pool, &secrets, &principal.id)
        .await
        .map(Json)
}

/// Une conversation de l'historique (`GET /api/v1/ai/conversations/{id}`).
pub async fn get_conversation(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Path(id): Path<String>,
) -> AppResult<Json<AiConversation>> {
    auth::require(&principal, Permission::ViewOverview)?;
    let secrets = SecretBox::open(&state.config.config_dir)?;
    history::load(&state.pool, &secrets, &principal.id, &id)
        .await
        .map(Json)
}

/// Supprime une conversation (`DELETE /api/v1/ai/conversations/{id}`).
pub async fn delete_conversation(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    auth::require(&principal, Permission::ViewOverview)?;
    history::delete(&state.pool, &principal.id, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Vide l'historique du compte (`DELETE /api/v1/ai/conversations`).
pub async fn delete_conversations(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
) -> AppResult<StatusCode> {
    auth::require(&principal, Permission::ViewOverview)?;
    history::delete_all(&state.pool, &principal.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Lance une conversation en tâche de fond (`POST /api/v1/ai/runs`) ; l'avancement se lit en SSE.
pub async fn start_run(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Json(request): Json<AiChatRequest>,
) -> AppResult<(StatusCode, Json<AiRunStarted>)> {
    auth::require(&principal, Permission::ViewOverview)?;
    validate_chat_request(&request)?;
    let ai_cfg = settings::load_effective(&state.pool, &state.config).await;
    if !ai_cfg.enabled || !ai_cfg.as_client_status().available {
        return Err(AppError::Conflict);
    }
    let (run_id, run) = state
        .ai_runs
        .create(&principal.id)
        .ok_or(AppError::RateLimited)?;
    let task_run = Arc::clone(&run);
    let question = request
        .messages
        .last()
        .map(|message| message.content.clone())
        .unwrap_or_default();
    let conversation_id = request.conversation_id.clone();
    let handle = tokio::spawn(async move {
        let last = match converse(&state, &principal, request, Some(&task_run)).await {
            Ok(mut response) => {
                response.conversation_id = save_exchange(
                    &state,
                    &principal,
                    conversation_id.as_deref(),
                    &question,
                    &response.reply,
                )
                .await;
                AiRunEvent::Done { response }
            }
            Err(error) => AiRunEvent::Failed {
                message: failure_message(&error).to_owned(),
            },
        };
        task_run.publish(last);
    });
    run.attach_abort(handle.abort_handle());
    Ok((StatusCode::ACCEPTED, Json(AiRunStarted { run_id })))
}

/// Évènements d'un run (SSE) ; reprend après `Last-Event-ID` en cas de reconnexion.
pub async fn run_events(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Path(run_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let run = state
        .ai_runs
        .get(&run_id, &principal.id)
        .ok_or(AppError::NotFound)?;
    let first = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map_or(0, |last| last.saturating_add(1));
    let stream = futures_util::stream::unfold((run, first), |(run, index)| async move {
        let event = run.next_event(index).await?;
        let sse = Event::default()
            .id(index.to_string())
            .json_data(&event)
            .unwrap_or_else(|_| Event::default().data("{}"));
        Some((Ok(sse), (run, index + 1)))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEP_ALIVE)))
}

/// Réponse de l'utilisateur à une demande de confirmation (`POST /api/v1/ai/runs/{id}/actions/{action}`).
pub async fn decide_action(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Path((run_id, action_id)): Path<(String, String)>,
    Json(decision): Json<AiActionDecision>,
) -> AppResult<StatusCode> {
    let run = state
        .ai_runs
        .get(&run_id, &principal.id)
        .ok_or(AppError::NotFound)?;
    if run.resolve(&action_id, decision.approve) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// Interrompt un run (`DELETE /api/v1/ai/runs/{id}`).
pub async fn cancel_run(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
    Path(run_id): Path<String>,
) -> AppResult<StatusCode> {
    let run = state
        .ai_runs
        .get(&run_id, &principal.id)
        .ok_or(AppError::NotFound)?;
    run.cancel(AiRunEvent::Failed {
        message: "Conversation interrompue.".to_owned(),
    });
    Ok(StatusCode::NO_CONTENT)
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
        ai::{AiActionDecision, AiChatMessage, AiContextKind, AiContextObject, AiProvider},
    };
    use serde_json::Value;
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
        assert_eq!(
            test_body
                .available_models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["mock-model"]
        );

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
                context: None,
                conversation_id: None,
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
                context: None,
                conversation_id: None,
            })?))?;
        let worker_res = app.oneshot(worker_req).await?;
        assert_eq!(worker_res.status(), StatusCode::FORBIDDEN);

        server_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn runs_stream_tool_steps_then_the_final_reply_and_stay_private() -> anyhow::Result<()> {
        let mock_llm = axum::Router::new().route(
            "/v1/chat/completions",
            post(|Json(body): Json<Value>| async move {
                let messages = body["messages"].as_array().cloned().unwrap_or_default();
                let system = messages
                    .first()
                    .and_then(|m| m["content"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                let after_tool = messages.iter().any(|m| m["role"] == "tool");
                if after_tool {
                    let seen = system.contains("flux `f1`");
                    Json(json!({ "choices": [{ "message": {
                        "role": "assistant",
                        "content": format!("Contexte reçu : {seen}")
                    }}]}))
                } else {
                    Json(json!({ "choices": [{ "message": {
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": { "name": "list_libraries", "arguments": "{}" }
                        }]
                    }}]}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mock_addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_llm).await;
        });

        let (app, state, _dir) = test_app().await?;
        let admin = issue_key_header(&state.pool, "run-admin", Role::Admin).await?;
        let other = issue_key_header(&state.pool, "run-other", Role::Admin).await?;
        let call = |method: &'static str, uri: String, auth: String, body: Body| {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", auth)
                .header("content-type", "application/json")
                .body(body);
            let app = app.clone();
            async move { anyhow::Ok(app.oneshot(request?).await?) }
        };
        let settings = Body::from(serde_json::to_vec(&UpdateAiSettingsRequest {
            enabled: true,
            provider: AiProvider::OpenAiCompatible,
            base_url: format!("http://{mock_addr}/v1"),
            model: "mock-model".into(),
            api_key: Some("sk-secret-value".into()),
            allow_write_tools: false,
            max_tool_rounds: 3,
        })?);
        let saved = call("PUT", "/api/v1/ai/settings".into(), admin.clone(), settings).await?;
        assert_eq!(saved.status(), StatusCode::OK);

        let request = AiChatRequest {
            messages: vec![AiChatMessage {
                role: AiChatRole::User,
                content: "Que contient ce flux ?".into(),
            }],
            context_hint: None,
            conversation_id: None,
            context: Some(AiContext {
                screen: "/app/flows/f1".into(),
                object: Some(AiContextObject {
                    kind: AiContextKind::Flow,
                    id: "f1".into(),
                }),
            }),
        };
        let started = call(
            "POST",
            "/api/v1/ai/runs".into(),
            admin.clone(),
            Body::from(serde_json::to_vec(&request)?),
        )
        .await?;
        assert_eq!(started.status(), StatusCode::ACCEPTED);
        let started: AiRunStarted =
            serde_json::from_slice(&axum::body::to_bytes(started.into_body(), 4096).await?)?;

        let events_uri = format!("/api/v1/ai/runs/{}/events", started.run_id);
        let hidden = call("GET", events_uri.clone(), other.clone(), Body::empty()).await?;
        assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
        let hidden_cancel = call(
            "DELETE",
            format!("/api/v1/ai/runs/{}", started.run_id),
            other,
            Body::empty(),
        )
        .await?;
        assert_eq!(hidden_cancel.status(), StatusCode::NOT_FOUND);

        let stream = call("GET", events_uri.clone(), admin.clone(), Body::empty()).await?;
        assert_eq!(stream.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(stream.into_body(), 1 << 20).await?;
        let events = String::from_utf8(raw.to_vec())?
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|data| serde_json::from_str::<AiRunEvent>(data.trim()))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(matches!(
            &events[0],
            AiRunEvent::ToolStarted { tool_name } if tool_name == "list_libraries"
        ));
        assert!(matches!(
            &events[1],
            AiRunEvent::ToolFinished { trace } if trace.outcome == "ok"
        ));
        match &events[2] {
            AiRunEvent::Done { response } => {
                assert_eq!(response.reply, "Contexte reçu : true");
                assert_eq!(response.rounds_used, 2);
            }
            other => anyhow::bail!("évènement final inattendu : {other:?}"),
        }
        assert_eq!(events.len(), 3);

        // Une reconnexion reprend après le dernier évènement lu.
        let resumed = {
            let request = Request::builder()
                .uri(events_uri)
                .header("authorization", &admin)
                .header("last-event-id", "1")
                .body(Body::empty())?;
            app.clone().oneshot(request).await?
        };
        let raw = axum::body::to_bytes(resumed.into_body(), 1 << 20).await?;
        let text = String::from_utf8(raw.to_vec())?;
        assert_eq!(text.matches("data:").count(), 1);
        assert!(text.contains("\"type\":\"done\""));

        server_task.abort();
        Ok(())
    }

    /// Lit un flux SSE jusqu'à ce que `until` soit vrai sur les évènements reçus, ou jusqu'à sa fin.
    async fn read_events(
        stream: &mut (impl futures_util::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Unpin),
        until: impl Fn(&[AiRunEvent]) -> bool,
    ) -> anyhow::Result<Vec<AiRunEvent>> {
        use futures_util::StreamExt;
        let mut raw = String::new();
        let parse = |raw: &str| -> anyhow::Result<Vec<AiRunEvent>> {
            raw.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|data| Ok(serde_json::from_str::<AiRunEvent>(data.trim())?))
                .collect()
        };
        while let Some(chunk) = stream.next().await {
            raw.push_str(&String::from_utf8(chunk?.to_vec())?);
            if until(&parse(&raw)?) {
                break;
            }
        }
        parse(&raw)
    }

    #[tokio::test]
    async fn writes_wait_for_confirmation_and_declining_skips_the_tool() -> anyhow::Result<()> {
        let mock_llm = axum::Router::new().route(
            "/v1/chat/completions",
            post(|Json(body): Json<Value>| async move {
                let messages = body["messages"].as_array().cloned().unwrap_or_default();
                let tool_result = messages
                    .iter()
                    .find(|m| m["role"] == "tool")
                    .and_then(|m| m["content"].as_str())
                    .map(str::to_owned);
                match tool_result {
                    Some(result) => Json(json!({ "choices": [{ "message": {
                        "role": "assistant",
                        "content": format!("Résultat de l'outil : {result}")
                    }}]})),
                    None => Json(json!({ "choices": [{ "message": {
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "upsert_local_script",
                                "arguments": "{\"name\":\"Nettoyage\",\"interpreter\":\"sh\",\"code\":\"echo bonjour\",\"token\":\"ne-pas-montrer\"}"
                            }
                        }]
                    }}]})),
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mock_addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_llm).await;
        });

        let (app, state, _dir) = test_app().await?;
        let admin = issue_key_header(&state.pool, "confirm-admin", Role::Admin).await?;
        let call = |method: &'static str, uri: String, body: Body| {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", &admin)
                .header("content-type", "application/json")
                .body(body);
            let app = app.clone();
            async move { anyhow::Ok(app.oneshot(request?).await?) }
        };
        let settings = Body::from(serde_json::to_vec(&UpdateAiSettingsRequest {
            enabled: true,
            provider: AiProvider::OpenAiCompatible,
            base_url: format!("http://{mock_addr}/v1"),
            model: "mock-model".into(),
            api_key: Some("sk-secret-value".into()),
            allow_write_tools: true,
            max_tool_rounds: 3,
        })?);
        assert_eq!(
            call("PUT", "/api/v1/ai/settings".into(), settings)
                .await?
                .status(),
            StatusCode::OK
        );
        let question = || -> anyhow::Result<Body> {
            Ok(Body::from(serde_json::to_vec(&AiChatRequest {
                messages: vec![AiChatMessage {
                    role: AiChatRole::User,
                    content: "Crée un script de nettoyage".into(),
                }],
                context_hint: None,
                context: None,
                conversation_id: None,
            })?))
        };

        // Sans run, personne ne peut confirmer : l'écriture n'est même pas proposée au modèle.
        let chat = call("POST", "/api/v1/ai/chat".into(), question()?).await?;
        let chat: AiChatResponse =
            serde_json::from_slice(&axum::body::to_bytes(chat.into_body(), 65_536).await?)?;
        assert_eq!(chat.tool_calls[0].outcome, "blocked_write_mode");

        for approve in [true, false] {
            let started = call("POST", "/api/v1/ai/runs".into(), question()?).await?;
            let started: AiRunStarted =
                serde_json::from_slice(&axum::body::to_bytes(started.into_body(), 4096).await?)?;
            let stream = call(
                "GET",
                format!("/api/v1/ai/runs/{}/events", started.run_id),
                Body::empty(),
            )
            .await?;
            let mut stream = stream.into_body().into_data_stream();
            let waiting = read_events(&mut stream, |events| {
                events
                    .iter()
                    .any(|e| matches!(e, AiRunEvent::Confirmation { .. }))
            })
            .await?;
            let Some(AiRunEvent::Confirmation {
                action_id,
                tool_name,
                summary,
            }) = waiting.last().cloned()
            else {
                anyhow::bail!("confirmation attendue, reçu {waiting:?}");
            };
            assert_eq!(tool_name, "upsert_local_script");
            assert!(summary.contains("echo bonjour") && summary.contains("Nettoyage"));
            assert!(!summary.contains("ne-pas-montrer"));
            // Le run est suspendu : aucun outil n'a fini.
            assert!(
                !waiting
                    .iter()
                    .any(|e| matches!(e, AiRunEvent::ToolFinished { .. }))
            );

            let decision = Body::from(serde_json::to_vec(&AiActionDecision { approve })?);
            let uri = format!("/api/v1/ai/runs/{}/actions/{action_id}", started.run_id);
            assert_eq!(
                call("POST", uri.clone(), decision).await?.status(),
                StatusCode::NO_CONTENT
            );
            // Une action déjà résolue ne peut pas être rejouée.
            let again = Body::from(serde_json::to_vec(&AiActionDecision { approve: true })?);
            assert_eq!(
                call("POST", uri, again).await?.status(),
                StatusCode::NOT_FOUND
            );

            let mut all = waiting;
            all.extend(read_events(&mut stream, |_| false).await?);
            let resolved = all.iter().any(|e| {
                matches!(e, AiRunEvent::ActionResolved { approved, .. } if *approved == approve)
            });
            assert!(resolved, "ActionResolved({approve}) attendu dans {all:?}");
            let finished = all
                .iter()
                .find_map(|e| match e {
                    AiRunEvent::ToolFinished { trace } => Some(trace.clone()),
                    _ => None,
                })
                .ok_or_else(|| anyhow::anyhow!("outil non terminé"))?;
            if approve {
                assert_eq!(finished.outcome, "ok", "{all:?}");
            } else {
                assert_eq!(finished.outcome, "declined");
                assert!(matches!(
                    all.last(),
                    Some(AiRunEvent::Done { response }) if response.reply.contains("refusée")
                ));
            }
        }

        server_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn runs_are_saved_in_a_private_history_that_can_be_continued_and_deleted()
    -> anyhow::Result<()> {
        let mock_llm = axum::Router::new().route(
            "/v1/chat/completions",
            post(|| async {
                Json(json!({ "choices": [{ "message": {
                    "role": "assistant", "content": "Voici la réponse."
                }}]}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mock_addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_llm).await;
        });

        let (app, state, _dir) = test_app().await?;
        let alice = issue_key_header(&state.pool, "history-alice", Role::Admin).await?;
        let bob = issue_key_header(&state.pool, "history-bob", Role::Admin).await?;
        let call = |method: &'static str, uri: String, auth: &str, body: Body| {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", auth)
                .header("content-type", "application/json")
                .body(body);
            let app = app.clone();
            async move { anyhow::Ok(app.oneshot(request?).await?) }
        };
        let settings = Body::from(serde_json::to_vec(&UpdateAiSettingsRequest {
            enabled: true,
            provider: AiProvider::OpenAiCompatible,
            base_url: format!("http://{mock_addr}/v1"),
            model: "mock-model".into(),
            api_key: Some("sk-secret-value".into()),
            allow_write_tools: false,
            max_tool_rounds: 2,
        })?);
        assert_eq!(
            call("PUT", "/api/v1/ai/settings".into(), &alice, settings)
                .await?
                .status(),
            StatusCode::OK
        );

        let ask = |conversation_id: Option<String>, text: &str| -> anyhow::Result<Body> {
            Ok(Body::from(serde_json::to_vec(&AiChatRequest {
                messages: vec![AiChatMessage {
                    role: AiChatRole::User,
                    content: text.into(),
                }],
                context_hint: None,
                context: None,
                conversation_id,
            })?))
        };
        let run_to_end = |conversation_id: Option<String>, text: &'static str| {
            let call = &call;
            let alice = alice.clone();
            let body = ask(conversation_id, text);
            async move {
                let started = call("POST", "/api/v1/ai/runs".into(), &alice, body?).await?;
                let started: AiRunStarted = serde_json::from_slice(
                    &axum::body::to_bytes(started.into_body(), 4096).await?,
                )?;
                let stream = call(
                    "GET",
                    format!("/api/v1/ai/runs/{}/events", started.run_id),
                    &alice,
                    Body::empty(),
                )
                .await?;
                let mut stream = stream.into_body().into_data_stream();
                let events = read_events(&mut stream, |_| false).await?;
                match events.last() {
                    Some(AiRunEvent::Done { response }) => Ok(response.clone()),
                    other => anyhow::bail!("fin inattendue : {other:?}"),
                }
            }
        };

        let first = run_to_end(None, "Pourquoi la tâche 12 échoue ?").await?;
        let id = first
            .conversation_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("conversation non enregistrée"))?;
        let second = run_to_end(Some(id.clone()), "Et la suivante ?").await?;
        assert_eq!(second.conversation_id.as_deref(), Some(id.as_str()));

        let listed = call(
            "GET",
            "/api/v1/ai/conversations".into(),
            &alice,
            Body::empty(),
        )
        .await?;
        let listed: AiConversationList =
            serde_json::from_slice(&axum::body::to_bytes(listed.into_body(), 65_536).await?)?;
        assert_eq!(listed.conversations.len(), 1);
        assert_eq!(
            listed.conversations[0].title,
            "Pourquoi la tâche 12 échoue ?"
        );

        let uri = format!("/api/v1/ai/conversations/{id}");
        let loaded = call("GET", uri.clone(), &alice, Body::empty()).await?;
        let loaded: AiConversation =
            serde_json::from_slice(&axum::body::to_bytes(loaded.into_body(), 65_536).await?)?;
        let texts: Vec<&str> = loaded.messages.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Pourquoi la tâche 12 échoue ?",
                "Voici la réponse.",
                "Et la suivante ?",
                "Voici la réponse."
            ]
        );

        // Un autre compte ne voit rien et ne peut rien supprimer.
        let bobs = call(
            "GET",
            "/api/v1/ai/conversations".into(),
            &bob,
            Body::empty(),
        )
        .await?;
        let bobs: AiConversationList =
            serde_json::from_slice(&axum::body::to_bytes(bobs.into_body(), 65_536).await?)?;
        assert!(bobs.conversations.is_empty());
        assert_eq!(
            call("GET", uri.clone(), &bob, Body::empty())
                .await?
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call("DELETE", uri.clone(), &bob, Body::empty())
                .await?
                .status(),
            StatusCode::NOT_FOUND
        );

        assert_eq!(
            call("DELETE", uri.clone(), &alice, Body::empty())
                .await?
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call("GET", uri, &alice, Body::empty()).await?.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(
                "DELETE",
                "/api/v1/ai/conversations".into(),
                &alice,
                Body::empty()
            )
            .await?
            .status(),
            StatusCode::NO_CONTENT
        );

        server_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn chatgpt_oauth_chat_requires_connection_sends_account_and_can_disconnect()
    -> anyhow::Result<()> {
        let mock_llm = axum::Router::new()
            .route(
                "/models",
                axum::routing::get(|headers: axum::http::HeaderMap| async move {
                    assert_eq!(
                        headers
                            .get("chatgpt-account-id")
                            .and_then(|v| v.to_str().ok()),
                        Some("acct-1")
                    );
                    Json(json!({ "models": [
                        { "slug": "interdit", "visibility": "hide", "priority": 0 },
                        { "slug": "gpt-second", "visibility": "list", "priority": 2 },
                        { "slug": "gpt-auto", "visibility": "list", "priority": 1 },
                    ]}))
                }),
            )
            .route(
            "/responses",
            post(
                |headers: axum::http::HeaderMap, Json(body): Json<Value>| async move {
                    let header = |name: &str| {
                        headers
                            .get(name)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_owned()
                    };
                    assert_eq!(header("authorization"), "Bearer access-1");
                    assert_eq!(header("chatgpt-account-id"), "acct-1");
                    assert_eq!(body["stream"], true);
                    assert_eq!(body["store"], false);
                    // Modèle vide : le premier modèle proposé au compte est utilisé.
                    assert_eq!(body["model"], "gpt-auto");
                    assert!(body["instructions"].as_str().is_some_and(|s| !s.is_empty()));
                    let events = [
                        json!({
                            "type": "response.output_item.done",
                            "item": { "type": "message", "content": [{ "type": "output_text", "text": "Bonjour ChatGPT" }] }
                        }),
                        json!({ "type": "response.completed" }),
                    ];
                    let sse: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
                    ([("content-type", "text/event-stream")], sse)
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mock_addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_llm).await;
        });

        let (app, state, _dir) = test_app().await?;
        let admin = issue_key_header(&state.pool, "oauth-admin", Role::Admin).await?;
        let worker = issue_key_header(&state.pool, "oauth-worker", Role::Worker).await?;
        let call = |method: &'static str, uri: &'static str, auth: &str, body: Body| {
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", auth)
                .header("content-type", "application/json")
                .body(body);
            let app = app.clone();
            async move { anyhow::Ok(app.oneshot(request?).await?) }
        };
        let chat_body = || -> anyhow::Result<Body> {
            Ok(Body::from(serde_json::to_vec(&AiChatRequest {
                messages: vec![AiChatMessage {
                    role: AiChatRole::User,
                    content: "Salut".into(),
                }],
                context_hint: None,
                context: None,
                conversation_id: None,
            })?))
        };

        let settings = Body::from(serde_json::to_vec(&UpdateAiSettingsRequest {
            enabled: true,
            provider: AiProvider::ChatGptOauth,
            base_url: format!("http://{mock_addr}"),
            model: String::new(),
            api_key: None,
            allow_write_tools: false,
            max_tool_rounds: 3,
        })?);
        assert_eq!(
            call("PUT", "/api/v1/ai/settings", &admin, settings)
                .await?
                .status(),
            StatusCode::OK
        );

        // Aucune connexion : l'assistant n'est pas disponible.
        assert_eq!(
            call("POST", "/api/v1/ai/chat", &admin, chat_body()?)
                .await?
                .status(),
            StatusCode::CONFLICT
        );

        let tokens = oauth::OauthTokens {
            access_token: "access-1".into(),
            refresh_token: "refresh-1".into(),
            expires_at: 4_000_000_000,
            account_id: Some("acct-1".into()),
            email: Some("remi@example.test".into()),
            plan: Some("plus".into()),
        };
        settings::save_oauth_tokens(&state.pool, &state.config, Some(&tokens)).await?;

        let view = call("GET", "/api/v1/ai/settings", &admin, Body::empty()).await?;
        let view: AiSettingsView =
            serde_json::from_slice(&axum::body::to_bytes(view.into_body(), 16_384).await?)?;
        assert_eq!(
            view.oauth_account.and_then(|a| a.email).as_deref(),
            Some("remi@example.test")
        );

        let chat = call("POST", "/api/v1/ai/chat", &admin, chat_body()?).await?;
        assert_eq!(chat.status(), StatusCode::OK);
        let chat: AiChatResponse =
            serde_json::from_slice(&axum::body::to_bytes(chat.into_body(), 65_536).await?)?;
        assert_eq!(chat.reply, "Bonjour ChatGPT");
        assert_eq!(chat.provider, AiProvider::ChatGptOauth);
        assert_eq!(chat.model, "gpt-auto");

        let test_body = Body::from(serde_json::to_vec(&AiTestRequest {
            provider: AiProvider::ChatGptOauth,
            base_url: format!("http://{mock_addr}"),
            model: String::new(),
            api_key: None,
        })?);
        let tested = call("POST", "/api/v1/ai/test", &admin, test_body).await?;
        let tested: AiTestResponse =
            serde_json::from_slice(&axum::body::to_bytes(tested.into_body(), 16_384).await?)?;
        assert!(tested.reachable);
        assert_eq!(
            tested
                .available_models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["gpt-auto", "gpt-second"]
        );

        // Seul un administrateur gère la connexion.
        assert_eq!(
            call("POST", "/api/v1/ai/oauth/start", &worker, Body::empty())
                .await?
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call("DELETE", "/api/v1/ai/oauth", &worker, Body::empty())
                .await?
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call("DELETE", "/api/v1/ai/oauth", &admin, Body::empty())
                .await?
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call("POST", "/api/v1/ai/chat", &admin, chat_body()?)
                .await?
                .status(),
            StatusCode::CONFLICT
        );
        server_task.abort();
        Ok(())
    }
}
