//! Assistant IA intégré au central : orchestration LLM multi-fournisseurs branchée sur les outils MCP (ADR 0015).

pub mod diagnosis;
pub mod health;
pub mod history;
pub mod oauth;
mod preview;
mod provider;
pub mod run;
pub mod settings;
mod streaming;
pub mod usage;

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
use forgearr_shared::{
    Permission,
    ai::{
        AiActionDecision, AiChatRequest, AiChatResponse, AiChatRole, AiClientStatus, AiContext,
        AiConversation, AiConversationList, AiDiagnosis, AiDiagnosisSettings, AiHealth,
        AiOauthDeviceCode, AiOauthPollResponse, AiProvider, AiQuotaSettings, AiRunEvent,
        AiRunStarted, AiSettingsView, AiTestRequest, AiTestResponse, AiToolCallTrace,
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
use streaming::DeltaSink;

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
        "Tu es l'assistant intégré de {} (orchestrateur de traitement média). \
         Réponds en français, de façon claire, concise et factuelle à partir des données obtenues par tes outils.\n\
         Utilisateur actif : {} (rôle : {}).\n\
         Périmètre : {}\n\
         Consignes : privilégie `diagnose_job` pour analyser une tâche en échec, `simulate_flow` et `validate_flow` pour vérifier un flux avant sauvegarde, et `get_job_stats` / `list_workers` / `audit` pour l'état global. Ne réclame et n'invente jamais de secret.",
        forgearr_shared::brand::NAME,
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

/// Premier modèle proposé au compte, quand aucun modèle n'est choisi (comme Codex CLI) ;
/// gardé quelques minutes pour ne pas lire `/models` à chaque question.
async fn first_available_model(
    state: &AppState,
    endpoint: &ProviderEndpoint,
    policy: OutboundPolicy,
) -> AppResult<String> {
    let key = format!(
        "{}|{}|{}",
        endpoint.provider,
        endpoint.base_url,
        endpoint.account_id.as_deref().unwrap_or_default()
    );
    if let Some(model) = state.ai_models.get(&key) {
        return Ok(model);
    }
    let models = provider::list_chatgpt_models(endpoint, policy)
        .await
        .map_err(|err| {
            tracing::warn!(provider = %endpoint.provider, error = %err, "liste des modèles indisponible");
            state.ai_health.record_failure(&err);
            AppError::Internal
        })?;
    let first = models
        .into_iter()
        .next()
        .map(|model| model.id)
        .ok_or_else(|| {
            tracing::warn!(provider = %endpoint.provider, "aucun modèle proposé à ce compte");
            AppError::Internal
        })?;
    state.ai_models.put(&key, &first);
    Ok(first)
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
    let polled = oauth::poll(&state.pool, &state.config, oauth::OAUTH_ISSUER, policy).await?;
    state.ai_models.clear();
    Ok(Json(polled))
}

/// Oublie la connexion OAuth enregistrée (`DELETE /api/v1/ai/oauth`).
pub async fn oauth_disconnect(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<StatusCode> {
    oauth::disconnect(&state.pool, &state.config).await?;
    state.ai_models.clear();
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
    state.ai_models.clear();
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
    let tested = provider::test_connection(&endpoint, policy).await;
    if tested.reachable {
        // Un test réussi rafraîchit aussi le modèle choisi automatiquement.
        state.ai_models.clear();
    }
    Ok(Json(tested))
}

/// État du fournisseur et consommation du jour (`GET /api/v1/ai/health`).
pub async fn get_health(
    State(state): State<AppState>,
    AdminAuth(principal): AdminAuth,
) -> AppResult<Json<AiHealth>> {
    let (last_success, last_error) = state.ai_health.snapshot();
    let (requests_today, your_requests_today) =
        usage::requests_today(&state.pool, &principal.id).await?;
    Ok(Json(AiHealth {
        last_success_at: last_success.as_ref().map(|(at, _)| at.clone()),
        last_latency_ms: last_success.map(|(_, latency)| latency),
        last_error,
        quota: usage::load_quotas(&state.pool).await,
        requests_today,
        your_requests_today,
    }))
}

/// Diagnostic proactif des échecs de tâches (`GET /api/v1/ai/diagnosis`).
pub async fn get_diagnosis(
    State(state): State<AppState>,
    AnyAuth(principal): AnyAuth,
) -> AppResult<Json<AiDiagnosis>> {
    auth::require(&principal, Permission::ViewJobs)?;
    Ok(Json(diagnosis::current(&state).await))
}

/// Active ou coupe le diagnostic proactif (`PUT /api/v1/ai/diagnosis`).
pub async fn update_diagnosis(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<AiDiagnosisSettings>,
) -> AppResult<Json<AiDiagnosis>> {
    diagnosis::set_enabled(&state.pool, request.enabled).await?;
    Ok(Json(diagnosis::current(&state).await))
}

/// Recalcule la synthèse maintenant (`POST /api/v1/ai/diagnosis/refresh`).
pub async fn refresh_diagnosis(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<AiDiagnosis>> {
    diagnosis::refresh(&state).await.map(Json)
}

/// Plafonds quotidiens (`GET /api/v1/ai/quotas`).
pub async fn get_quotas(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<AiQuotaSettings>> {
    Ok(Json(usage::load_quotas(&state.pool).await))
}

/// Règle les plafonds quotidiens (`PUT /api/v1/ai/quotas`) ; `0` laisse la limite ouverte.
pub async fn update_quotas(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<AiQuotaSettings>,
) -> AppResult<Json<AiQuotaSettings>> {
    usage::save_quotas(&state.pool, request).await.map(Json)
}

/// Vérifie que l'assistant est utilisable et compte la question dans les quotas du jour.
async fn admit(state: &AppState, principal: &Principal) -> AppResult<()> {
    admit_owner(state, &principal.id).await
}

/// Comme `admit`, pour un propriétaire de quota qui n'est pas un compte (diagnostic proactif).
async fn admit_owner(state: &AppState, owner: &str) -> AppResult<()> {
    let ai_cfg = settings::load_effective(&state.pool, &state.config).await;
    if !ai_cfg.enabled || !ai_cfg.as_client_status().available {
        return Err(AppError::Conflict);
    }
    let quotas = usage::load_quotas(&state.pool).await;
    usage::record_question(&state.pool, owner, quotas).await
}

/// Un tour de modèle, dont le résultat alimente l'état du fournisseur.
async fn timed_turn(
    state: &AppState,
    endpoint: &ProviderEndpoint,
    system_prompt: &str,
    entries: &[ConversationEntry],
    tools: &[McpTool],
    policy: OutboundPolicy,
    on_delta: DeltaSink<'_>,
) -> Result<ModelTurnResponse, String> {
    let started = Instant::now();
    let result =
        provider::complete_turn(endpoint, system_prompt, entries, tools, policy, on_delta).await;
    match &result {
        Ok(_) => state
            .ai_health
            .record_success(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
        Err(error) => state.ai_health.record_failure(error),
    }
    result
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
        AppError::RateLimited => {
            "Quota quotidien de questions atteint (réglable dans Paramètres > Assistant IA)."
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
    admit(&state, &principal).await?;
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
        endpoint.model = first_available_model(state, &endpoint, policy).await?;
    }
    let writes_allowed = ai_cfg.allow_write_tools && run.is_some();
    let tools = available_tools_for_assistant(principal, writes_allowed);
    let system_prompt = build_system_prompt(
        principal,
        writes_allowed,
        request.context_hint.as_deref(),
        request.context.as_ref(),
    );

    // Le texte n'est diffusé que pour un run : seul un run a quelqu'un qui l'écoute.
    let stream_text = |text: &str| {
        publish(
            run,
            AiRunEvent::Delta {
                text: text.to_owned(),
            },
        );
    };
    let on_delta: DeltaSink<'_> = run.map(|_| &stream_text as &(dyn Fn(&str) + Send + Sync));

    let mut entries = provider::initial_entries(&request.messages);
    let mut traces: Vec<AiToolCallTrace> = Vec::new();
    let mut rounds_used: u8 = 0;

    for _round in 0..ai_cfg.max_tool_rounds {
        rounds_used += 1;
        let turn = timed_turn(
            state,
            &endpoint,
            &system_prompt,
            &entries,
            &tools,
            policy,
            on_delta,
        )
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
    let final_reply = timed_turn(
        state,
        &endpoint,
        &system_prompt,
        &entries,
        &[],
        policy,
        on_delta,
    )
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
    let (run_id, run) = state
        .ai_runs
        .create(&principal.id)
        .ok_or(AppError::RateLimited)?;
    if let Err(error) = admit(&state, &principal).await {
        // Un run refusé ne doit pas compter parmi les runs actifs du compte.
        run.publish(AiRunEvent::Failed {
            message: failure_message(&error).to_owned(),
        });
        return Err(error);
    }
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
    use axum::response::IntoResponse;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use forgearr_shared::{
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
    async fn quotas_stop_questions_and_health_reports_the_provider_state() -> anyhow::Result<()> {
        let healthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let healthy_in_mock = Arc::clone(&healthy);
        let mock_llm = axum::Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let ok = healthy_in_mock.load(Ordering::SeqCst);
                async move {
                    if ok {
                        (
                            StatusCode::OK,
                            Json(json!({ "choices": [{ "message": {
                                "role": "assistant", "content": "Bonjour"
                            }}]})),
                        )
                    } else {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": "panne du fournisseur" })),
                        )
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
        let admin = issue_key_header(&state.pool, "quota-admin", Role::Admin).await?;
        let worker = issue_key_header(&state.pool, "quota-worker", Role::Worker).await?;
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
            call("PUT", "/api/v1/ai/settings", &admin, settings)
                .await?
                .status(),
            StatusCode::OK
        );
        let question = || -> anyhow::Result<Body> {
            Ok(Body::from(serde_json::to_vec(&AiChatRequest {
                messages: vec![AiChatMessage {
                    role: AiChatRole::User,
                    content: "Bonjour".into(),
                }],
                context_hint: None,
                context: None,
                conversation_id: None,
            })?))
        };
        let health = || async {
            let res = call("GET", "/api/v1/ai/health", &admin, Body::empty()).await?;
            anyhow::Ok(serde_json::from_slice::<AiHealth>(
                &axum::body::to_bytes(res.into_body(), 16_384).await?,
            )?)
        };

        // Les réglages de quota sont réservés aux administrateurs et bornés.
        let quotas = |per_user: u32, global: u32| {
            Body::from(
                serde_json::to_vec(&AiQuotaSettings {
                    per_user_daily: per_user,
                    global_daily: global,
                })
                .unwrap_or_default(),
            )
        };
        assert_eq!(
            call("PUT", "/api/v1/ai/quotas", &worker, quotas(1, 0))
                .await?
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                "PUT",
                "/api/v1/ai/quotas",
                &admin,
                quotas(usage::MAX_DAILY_QUOTA + 1, 0)
            )
            .await?
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call("PUT", "/api/v1/ai/quotas", &admin, quotas(2, 0))
                .await?
                .status(),
            StatusCode::OK
        );

        // Question 1 : le fournisseur est en panne, l'erreur est visible dans l'état.
        let failed = call("POST", "/api/v1/ai/chat", &admin, question()?).await?;
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let after_failure = health().await?;
        assert!(after_failure.last_success_at.is_none());
        assert!(
            after_failure
                .last_error
                .is_some_and(|e| e.message.contains("500"))
        );
        assert_eq!(after_failure.requests_today, 1);

        // Question 2 : le fournisseur répond ; le succès et sa latence sont enregistrés.
        healthy.store(true, Ordering::SeqCst);
        let ok = call("POST", "/api/v1/ai/chat", &admin, question()?).await?;
        assert_eq!(ok.status(), StatusCode::OK);
        let after_success = health().await?;
        assert!(after_success.last_success_at.is_some());
        assert!(after_success.last_latency_ms.is_some());
        assert!(
            after_success.last_error.is_some(),
            "la dernière erreur reste visible"
        );
        assert_eq!(after_success.your_requests_today, 2);
        assert_eq!(after_success.quota.per_user_daily, 2);

        // Question 3 : le plafond du compte est atteint, par le chat comme par un run.
        let limited = call("POST", "/api/v1/ai/chat", &admin, question()?).await?;
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        let limited_run = call("POST", "/api/v1/ai/runs", &admin, question()?).await?;
        assert_eq!(limited_run.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(health().await?.requests_today, 2);

        // Remise à zéro de la limite : les questions repassent.
        assert_eq!(
            call("PUT", "/api/v1/ai/quotas", &admin, quotas(0, 0))
                .await?
                .status(),
            StatusCode::OK
        );
        let again = call("POST", "/api/v1/ai/chat", &admin, question()?).await?;
        assert_eq!(again.status(), StatusCode::OK);

        server_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn proactive_diagnosis_is_opt_in_cheap_and_computed_once_per_batch_of_failures()
    -> anyhow::Result<()> {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in_mock = Arc::clone(&calls);
        let mock_llm = axum::Router::new().route(
            "/v1/chat/completions",
            post(move |Json(body): Json<Value>| {
                calls_in_mock.fetch_add(1, Ordering::SeqCst);
                let writes_offered = body["tools"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|tool| tool["function"]["name"] == "delete_job");
                async move {
                    Json(json!({ "choices": [{ "message": {
                        "role": "assistant",
                        "content": format!("Cause : disque plein. Écritures proposées : {writes_offered}")
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
        let admin = issue_key_header(&state.pool, "diag-admin", Role::Admin).await?;
        let worker = issue_key_header(&state.pool, "diag-worker", Role::Worker).await?;
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
        let read = |res: axum::response::Response| async move {
            anyhow::Ok(serde_json::from_slice::<AiDiagnosis>(
                &axum::body::to_bytes(res.into_body(), 65_536).await?,
            )?)
        };
        // Les réglages IA autorisent les écritures : le diagnostic ne doit pourtant jamais en proposer.
        let settings = Body::from(serde_json::to_vec(&UpdateAiSettingsRequest {
            enabled: true,
            provider: AiProvider::OpenAiCompatible,
            base_url: format!("http://{mock_addr}/v1"),
            model: "mock-model".into(),
            api_key: Some("sk-secret-value".into()),
            allow_write_tools: true,
            max_tool_rounds: 2,
        })?);
        assert_eq!(
            call("PUT", "/api/v1/ai/settings", &admin, settings)
                .await?
                .status(),
            StatusCode::OK
        );

        // Désactivé par défaut : ni synthèse, ni recalcul possible, ni appel au fournisseur.
        let initial =
            read(call("GET", "/api/v1/ai/diagnosis", &admin, Body::empty()).await?).await?;
        assert_eq!(initial, AiDiagnosis::default());
        assert_eq!(
            call(
                "POST",
                "/api/v1/ai/diagnosis/refresh",
                &admin,
                Body::empty()
            )
            .await?
            .status(),
            StatusCode::CONFLICT
        );
        diagnosis::tick(&state).await?;
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // Seul un administrateur règle le diagnostic.
        let enable = || {
            Body::from(
                serde_json::to_vec(&AiDiagnosisSettings { enabled: true }).unwrap_or_default(),
            )
        };
        assert_eq!(
            call("PUT", "/api/v1/ai/diagnosis", &worker, enable())
                .await?
                .status(),
            StatusCode::FORBIDDEN
        );
        let enabled = read(call("PUT", "/api/v1/ai/diagnosis", &admin, enable()).await?).await?;
        assert!(enabled.enabled && enabled.summary.is_none());

        // Sans échec, la synthèse le dit sans interroger le fournisseur.
        diagnosis::tick(&state).await?;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let quiet = read(
            call(
                "POST",
                "/api/v1/ai/diagnosis/refresh",
                &admin,
                Body::empty(),
            )
            .await?,
        )
        .await?;
        assert_eq!(quiet.summary.map(|s| s.failures), Some(0));
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // Un échec récent déclenche UN calcul, en lecture seule ; la boucle ne le refait pas.
        sqlx::query("INSERT INTO jobs (id, source_name, status, error_code) VALUES ('j1', 'film.mkv', 'failed', 'disk_full')")
            .execute(&state.pool)
            .await?;
        state.ai_diagnosis.clear_for_tests();
        diagnosis::tick(&state).await?;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let computed =
            read(call("GET", "/api/v1/ai/diagnosis", &admin, Body::empty()).await?).await?;
        let summary = computed
            .summary
            .ok_or_else(|| anyhow::anyhow!("synthèse absente"))?;
        assert_eq!(summary.failures, 1);
        assert!(summary.text.contains("disque plein"));
        assert!(summary.text.contains("Écritures proposées : false"));
        diagnosis::tick(&state).await?;
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Le travail de fond passe par les quotas.
        let usage_today = usage::requests_today(&state.pool, "system:diagnosis").await?;
        assert_eq!(usage_today.1, 1);

        server_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn runs_publish_the_reply_as_it_is_written_and_chat_stays_whole() -> anyhow::Result<()> {
        let mock_llm = axum::Router::new().route(
            "/v1/chat/completions",
            post(|Json(body): Json<Value>| async move {
                let pieces = [
                    "Bonjour ",
                    "tout ",
                    "le monde, voici une réponse assez longue pour être découpée.",
                ];
                if body["stream"] == true {
                    let sse: String = pieces
                        .iter()
                        .map(|piece| {
                            format!(
                                "data: {}\n\n",
                                json!({ "choices": [{ "delta": { "content": piece } }] })
                            )
                        })
                        .chain(std::iter::once("data: [DONE]\n\n".to_owned()))
                        .collect();
                    ([("content-type", "text/event-stream")], sse).into_response()
                } else {
                    Json(json!({ "choices": [{ "message": {
                        "role": "assistant", "content": pieces.concat()
                    }}]}))
                    .into_response()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mock_addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_llm).await;
        });

        let (app, state, _dir) = test_app().await?;
        let admin = issue_key_header(&state.pool, "stream-admin", Role::Admin).await?;
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
            allow_write_tools: false,
            max_tool_rounds: 2,
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
                    content: "Bonjour".into(),
                }],
                context_hint: None,
                context: None,
                conversation_id: None,
            })?))
        };
        let expected = "Bonjour tout le monde, voici une réponse assez longue pour être découpée.";

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
        let events = read_events(&mut stream, |_| false).await?;
        let streamed: String = events
            .iter()
            .filter_map(|event| match event {
                AiRunEvent::Delta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(streamed, expected);
        assert!(matches!(
            events.last(),
            Some(AiRunEvent::Done { response }) if response.reply == expected
        ));

        // Le chat sans run n'a personne à qui diffuser : réponse d'un bloc, sans flux.
        let chat = call("POST", "/api/v1/ai/chat".into(), question()?).await?;
        let chat: AiChatResponse =
            serde_json::from_slice(&axum::body::to_bytes(chat.into_body(), 65_536).await?)?;
        assert_eq!(chat.reply, expected);

        server_task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn chatgpt_oauth_chat_requires_connection_sends_account_and_can_disconnect()
    -> anyhow::Result<()> {
        let models_hits = Arc::new(AtomicUsize::new(0));
        let hits_in_mock = Arc::clone(&models_hits);
        let mock_llm = axum::Router::new()
            .route(
                "/models",
                axum::routing::get(
                    move |headers: axum::http::HeaderMap, uri: axum::http::Uri| {
                        hits_in_mock.fetch_add(1, Ordering::SeqCst);
                        async move {
                            assert_eq!(
                                headers
                                    .get("chatgpt-account-id")
                                    .and_then(|v| v.to_str().ok()),
                                Some("acct-1")
                            );
                            // Le backend Codex masque les modèles dont la version minimale n'est pas atteinte.
                            assert_eq!(
                                uri.query(),
                                Some("client_version=0.170.0"),
                                "version de client annoncée à /models"
                            );
                            Json(json!({ "models": [
                                { "slug": "interdit", "visibility": "hide", "priority": 0 },
                                { "slug": "gpt-second", "visibility": "list", "priority": 2 },
                                { "slug": "gpt-auto", "visibility": "list", "priority": 1 },
                            ]}))
                        }
                    },
                ),
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
        // Le modèle choisi automatiquement est gardé : une 2e question ne relit pas /models.
        assert_eq!(models_hits.load(Ordering::SeqCst), 1);
        let again = call("POST", "/api/v1/ai/chat", &admin, chat_body()?).await?;
        assert_eq!(again.status(), StatusCode::OK);
        assert_eq!(models_hits.load(Ordering::SeqCst), 1);

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
