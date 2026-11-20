mod accounts;
pub mod auth;
pub mod diagnostics;
pub(crate) mod flows;
pub(crate) mod installation;
pub(crate) mod jobs;
mod keys;
pub(crate) mod libraries;
pub(crate) mod local;
mod password_reset;
pub(crate) mod plugins;
pub(crate) mod recovery_kit;
pub(crate) mod session;
mod setup;
pub(crate) mod source_publication;
pub(crate) mod source_recovery;
pub(crate) mod source_recovery_remote;
pub(crate) mod source_relay;
pub(crate) mod system;
pub(crate) mod transfers;
mod worker_view;
use crate::{config::Config, domain::*, error::AppResult, repository};
pub use auth::{
    AdminAuth, AnyAuth, ManageAccountsAuth, ManageApiKeysAuth, ManageFlowsAuth, ManageJobsAuth,
    ManageLibrariesAuth, ManagePluginsAuth, ManageWorkersAuth, ViewFlowsAuth, ViewJobsAuth,
    ViewLibrariesAuth, ViewOverviewAuth, ViewPluginsAuth, ViewWorkersAuth,
};
use axum::{
    Router,
    extract::{ConnectInfo, Extension, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{
    collections::VecDeque,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

const SSE_REPLAY_CAPACITY: usize = 256;
const SSE_BROADCAST_CAPACITY: usize = 256;
const OUTBOUND_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Tampon d'historique borné pour la reprise SSE via `Last-Event-ID`.
#[derive(Clone)]
pub struct EventHistory {
    next_seq: Arc<AtomicU64>,
    buffer: Arc<tokio::sync::Mutex<VecDeque<(u64, JobEvent)>>>,
    tx: tokio::sync::broadcast::Sender<(u64, JobEvent)>,
}

impl Default for EventHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHistory {
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(SSE_BROADCAST_CAPACITY);
        Self {
            next_seq: Arc::new(AtomicU64::new(1)),
            buffer: Arc::new(tokio::sync::Mutex::new(VecDeque::with_capacity(
                SSE_REPLAY_CAPACITY,
            ))),
            tx,
        }
    }

    pub async fn publish(&self, event: JobEvent) -> u64 {
        let mut guard = self.buffer.lock().await;
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        guard.push_back((seq, event.clone()));
        while guard.len() > SSE_REPLAY_CAPACITY {
            guard.pop_front();
        }
        let _ = self.tx.send((seq, event));
        seq
    }

    pub async fn subscribe_from(
        &self,
        last_event_id: Option<u64>,
    ) -> (
        Vec<(u64, JobEvent)>,
        tokio::sync::broadcast::Receiver<(u64, JobEvent)>,
    ) {
        let guard = self.buffer.lock().await;
        let rx = self.tx.subscribe();
        let replay = match last_event_id {
            Some(since) => guard
                .iter()
                .filter(|(seq, _)| *seq > since)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        (replay, rx)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub http_client: reqwest::Client,
    pub catalog_cache: crate::plugins::CatalogCache,
    pub event_history: EventHistory,
    pub browse_broker: crate::filesystem_browser::RemoteBrowseBroker,
    pub source_relay: crate::source_relay::SourceRelayBroker,
    pub source_publication: crate::source_publication_broker::SourcePublicationBroker,
    pub source_inspection: crate::source_publication_broker::SourcePublicationBroker,
    pub pending_worker_setup: Arc<tokio::sync::Mutex<Option<installation::PendingWorkerSetup>>>,
    pub ai_runs: crate::ai::run::AiRuns,
    pub ai_health: crate::ai::health::AiHealthTracker,
    pub ai_models: crate::ai::health::ModelCache,
    pub ai_diagnosis: crate::ai::diagnosis::DiagnosisStore,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Arc<Config>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(OUTBOUND_HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self {
            pool,
            config,
            http_client,
            catalog_cache: crate::plugins::CatalogCache::new(),
            event_history: EventHistory::new(),
            browse_broker: crate::filesystem_browser::RemoteBrowseBroker::new(),
            source_relay: crate::source_relay::SourceRelayBroker::new(),
            source_publication: crate::source_publication_broker::SourcePublicationBroker::default(
            ),
            source_inspection: crate::source_publication_broker::SourcePublicationBroker::default(),
            pending_worker_setup: Arc::new(tokio::sync::Mutex::new(None)),
            ai_runs: crate::ai::run::AiRuns::new(),
            ai_health: crate::ai::health::AiHealthTracker::new(),
            ai_models: crate::ai::health::ModelCache::new(),
            ai_diagnosis: crate::ai::diagnosis::DiagnosisStore::new(),
        }
    }

    pub fn notify_job_event(
        &self,
        event_type: JobEventType,
        job_id: uuid::Uuid,
        status: JobStatus,
        error_code: Option<JobErrorCode>,
    ) {
        let event = JobEvent {
            event_type,
            job_id,
            status,
            timestamp: chrono::Utc::now(),
            progress: None,
            message: None,
            error_code,
        };
        let history = self.event_history.clone();
        tokio::spawn(async move {
            history.publish(event).await;
        });
    }

    pub fn notify_job_progress(&self, job_id: uuid::Uuid, progress: u32, message: Option<String>) {
        let event = JobEvent {
            event_type: JobEventType::JobProgress,
            job_id,
            status: JobStatus::Running,
            timestamp: chrono::Utc::now(),
            progress: Some(progress),
            message,
            error_code: None,
        };
        let history = self.event_history.clone();
        tokio::spawn(async move {
            history.publish(event).await;
        });
    }
}

/// Budget des routes JSON, y compris l'empreinte SHA-256 de `POST /api/v1/jobs`.
pub const JSON_API_TIMEOUT: Duration = Duration::from_secs(60);
/// Budget de l'assistant IA intégré (boucles multi-tours et modèles locaux Ollama).
pub const AI_CHAT_TIMEOUT: Duration = Duration::from_secs(180);
/// Budget des transferts de fichiers et du flux SSE, déjà borné dans `receive`.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(3600);
pub const JSON_BODY_LIMIT: usize = 16 * 1024;
/// Limite pour les scripts locaux (`MAX_SCRIPT_BYTES` = 512 Kio + enveloppe JSON), graphes de flows et modèles.
pub const LARGE_JSON_BODY_LIMIT: usize = 1024 * 1024;
/// Limite pour l'import base64 de ressources locales (`MAX_RESOURCE_BYTES` = 5 Mio ~ 6,7 Mio en base64).
pub const LOCAL_RESOURCE_BODY_LIMIT: usize = 10 * 1024 * 1024;

/// `/api/v1/ai/runs/{id}/events` : flux SSE d'une conversation de l'assistant.
fn is_ai_run_events_route(path: &str) -> bool {
    path.strip_prefix("/api/v1/ai/runs/")
        .and_then(|rest| rest.strip_suffix("/events"))
        .is_some_and(|id| !id.is_empty() && !id.contains('/'))
}

/// Routes dont la réponse peut durer bien au-delà d'un appel JSON.
pub fn is_long_running_route(path: &str) -> bool {
    if path == "/api/v1/jobs/events" || is_ai_run_events_route(path) {
        return true;
    }
    let Some(rest) = path.strip_prefix("/api/v1/jobs/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let id = parts.next();
    let action = parts.next();
    id.is_some_and(|segment| !segment.is_empty())
        && matches!(action, Some("source" | "output" | "result"))
        && parts.next().is_none()
}

fn json_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/openapi.yaml", get(openapi_yaml))
        .route(
            "/mcp",
            post(crate::mcp::handle_post)
                .get(crate::mcp::handle_get)
                .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT)),
        )
        .route("/api/v1/config", get(jobs::system_config))
        .route("/api/v1/files", get(jobs::files))
        .route(
            "/api/v1/libraries",
            get(libraries::list).post(libraries::create),
        )
        .route("/api/v1/libraries/browse", get(libraries::browse))
        .route(
            "/api/v1/libraries/{id}",
            get(libraries::get)
                .put(libraries::update)
                .delete(libraries::delete),
        )
        .route("/api/v1/libraries/{id}/files", get(libraries::file_history))
        .route(
            "/api/v1/libraries/{id}/files/{job}/reprocess",
            post(libraries::reprocess_file),
        )
        .route("/api/v1/jobs/{id}/placement", get(jobs::placement))
        .route(
            "/api/v1/jobs/{id}/notifications/retry",
            post(jobs::retry_notifications),
        )
        .route(
            "/api/v1/jobs/failures",
            get(jobs::failure_details).post(jobs::replay_failures),
        )
        .route("/api/v1/jobs/relays", get(jobs::relay_infos))
        .route(
            "/api/v1/worker/filesystem/pending",
            get(libraries::worker_pending_browse),
        )
        .route(
            "/api/v1/worker/filesystem",
            post(libraries::worker_report_browse)
                .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT)),
        )
        .route("/api/v1/flows", get(flows::list_flows))
        .route("/api/v1/flows/blocks", get(flows::list_blocks))
        .route("/api/v1/flows/validate", post(flows::validate_flow))
        .route("/api/v1/flows/{id}/versions", get(flows::list_versions))
        .route(
            "/api/v1/flows/{id}/versions/{version}",
            get(flows::get_version),
        )
        .route(
            "/api/v1/flows/{id}/versions/{version}/restore",
            post(flows::restore_version),
        )
        .route(
            "/api/v1/flows/{id}",
            get(flows::get_flow)
                .put(flows::save_flow)
                .delete(flows::delete_flow)
                .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT)),
        )
        .route("/api/v1/plugins", get(plugins::catalog))
        .route("/api/v1/plugins/{id}", get(plugins::plugin_detail))
        .route(
            "/api/v1/plugins/{id}/install",
            post(plugins::install_plugin),
        )
        .route(
            "/api/v1/plugins/repositories",
            get(plugins::list_repositories).post(plugins::add_repository),
        )
        .route(
            "/api/v1/plugins/repositories/{id}/toggle",
            put(plugins::toggle_repository),
        )
        .route(
            "/api/v1/plugins/repositories/{id}",
            get(plugins::repository_detail).delete(plugins::delete_repository),
        )
        .route(
            "/api/v1/plugins/repositories/{id}/sync",
            post(plugins::sync_repository),
        )
        .route(
            "/api/v1/plugins/{id}/settings",
            get(plugins::get_settings).put(plugins::save_settings),
        )
        .route("/api/v1/plugins/{id}/test", post(plugins::test_connection))
        .route("/api/v1/tools", get(plugins::tools))
        .route("/api/v1/tools/{id}", get(plugins::tool_detail))
        .route(
            "/api/v1/tools/{id}/distribution",
            get(plugins::tool_distribution),
        )
        .route("/api/v1/tools/{id}/install", post(plugins::install_tool))
        .route("/api/v1/worker/tools", get(plugins::worker_active_tools))
        .route(
            "/api/v1/worker/tools/{id}/distribution",
            get(plugins::worker_tool_distribution),
        )
        .route("/api/v1/packs/{id}", get(plugins::pack_detail))
        .route(
            "/api/v1/packs/{id}/installation",
            get(plugins::pack_installation),
        )
        .route("/api/v1/packs/{id}/install", post(plugins::install_pack))
        .route("/api/v1/local", get(local::snapshot))
        .route(
            "/api/v1/local/scripts",
            post(local::upsert_script)
                .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT)),
        )
        .route(
            "/api/v1/local/resources",
            post(local::upload_resource).layer(axum::extract::DefaultBodyLimit::max(
                LOCAL_RESOURCE_BODY_LIMIT,
            )),
        )
        .route(
            "/api/v1/local/block-templates",
            post(local::create_block_template)
                .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT)),
        )
        .route(
            "/api/v1/local/flow-templates",
            post(local::create_flow_template)
                .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT)),
        )
        .route("/api/v1/local/packs", post(local::create_pack))
        .route(
            "/api/v1/local/items/{id}",
            axum::routing::delete(local::delete_item),
        )
        .route("/api/v1/auth/session", get(session::current))
        .route("/api/v1/auth/login", post(session::login))
        .route("/api/v1/auth/forgot-password", post(password_reset::forgot))
        .route("/api/v1/auth/reset-password", post(password_reset::reset))
        .route("/api/v1/setup/status", get(setup::status))
        .route("/api/v1/setup/central", post(setup::setup_central))
        .route("/api/v1/setup/worker", post(installation::setup_worker))
        .route(
            "/api/v1/setup/worker-request",
            get(installation::setup_worker_request_status)
                .post(installation::start_setup_worker_request)
                .delete(installation::cancel_setup_worker_request),
        )
        .route("/api/v1/workers/enroll", post(installation::enroll))
        .route(
            "/api/v1/workers/enrollment-window",
            post(installation::set_enrollment_window),
        )
        .route(
            "/api/v1/workers/requests",
            get(installation::list_enrollment_requests)
                .post(installation::submit_enrollment_request),
        )
        .route(
            "/api/v1/workers/requests/{id}",
            get(installation::poll_enrollment_request),
        )
        .route(
            "/api/v1/workers/requests/{id}/approve",
            post(installation::approve_enrollment_request),
        )
        .route(
            "/api/v1/workers/requests/{id}/reject",
            post(installation::reject_enrollment_request),
        )
        .route("/api/v1/instance/status", get(installation::status))
        .route(
            crate::worker_view::OVERVIEW_PATH,
            get(worker_view::overview),
        )
        .route(crate::worker_view::JOBS_PATH, get(worker_view::jobs))
        .route(crate::worker_view::PLUGINS_PATH, get(worker_view::catalog))
        .route(crate::worker_view::LOGIN_PATH, post(worker_view::login))
        .route(
            crate::worker_view::ACCOUNTS_PATH,
            get(worker_view::accounts),
        )
        .route(
            crate::worker_view::CHANGE_PASSWORD_PATH,
            put(worker_view::change_password),
        )
        .route("/api/v1/me", get(session::me))
        .route("/api/v1/account/password", put(session::change_password))
        .route(
            "/api/v1/accounts",
            get(accounts::list).post(accounts::create),
        )
        .route("/api/v1/system/status", get(system::status))
        .route("/api/v1/system/settings", put(system::update_settings))
        .route(
            "/api/v1/system/timezone",
            get(system::timezone).put(system::update_timezone),
        )
        .route("/api/v1/system/secret-key/rotate", post(system::rotate_key))
        .route(
            "/api/v1/system/secret-key/recovery-kit",
            post(recovery_kit::export),
        )
        .route(
            "/api/v1/system/secret-key/restore",
            post(recovery_kit::restore),
        )
        .route("/api/v1/system/updates", get(system::updates))
        .route("/api/v1/system/hardware", get(system::hardware))
        .route("/api/v1/system/diagnostics", post(diagnostics::diagnostics))
        .route("/api/v1/accounts/{id}", put(accounts::update))
        .route("/api/v1/api-keys", get(keys::list).post(keys::create))
        .route("/api/v1/api-keys/{id}", axum::routing::delete(keys::revoke))
        .route("/api/v1/mcp/audit", get(crate::mcp::list_audit))
        .route("/api/v1/logs", get(crate::logs::list))
        .route("/api/v1/ai/status", get(crate::ai::client_status))
        .route(
            "/api/v1/ai/settings",
            get(crate::ai::get_settings).put(crate::ai::update_settings),
        )
        .route("/api/v1/ai/test", post(crate::ai::test_connection))
        .route("/api/v1/ai/health", get(crate::ai::get_health))
        .route(
            "/api/v1/ai/diagnosis",
            get(crate::ai::get_diagnosis).put(crate::ai::update_diagnosis),
        )
        .route(
            "/api/v1/ai/quotas",
            get(crate::ai::get_quotas).put(crate::ai::update_quotas),
        )
        .route("/api/v1/ai/oauth/start", post(crate::ai::oauth_start))
        .route("/api/v1/ai/oauth/poll", post(crate::ai::oauth_poll))
        .route(
            "/api/v1/ai/oauth",
            axum::routing::delete(crate::ai::oauth_disconnect),
        )
        .route("/api/v1/auth/logout", post(session::logout))
        .route("/api/v1/jobs", get(jobs::list).post(jobs::create))
        .route("/api/v1/jobs/stats", get(jobs::stats))
        .route("/api/v1/jobs/batch", post(jobs::batch))
        .route(
            "/api/v1/jobs/{id}",
            get(jobs::get_job).delete(jobs::delete_job),
        )
        .route(
            "/api/v1/jobs/{id}/steps",
            get(jobs::steps).post(jobs::report_steps),
        )
        .route("/api/v1/jobs/{id}/retry", post(jobs::retry))
        .route("/api/v1/jobs/{id}/cancel", post(jobs::cancel))
        .route("/api/v1/jobs/{id}/priority", put(jobs::update_priority))
        .route("/api/v1/workers", get(jobs::workers).post(register))
        .route(
            "/api/v1/workers/{id}",
            axum::routing::delete(jobs::delete_worker),
        )
        .route(
            "/api/v1/workers/{id}/settings",
            put(jobs::update_worker_settings),
        )
        .route("/api/v1/claims", post(claim))
        .route("/api/v1/jobs/{id}/heartbeat", post(jobs::heartbeat))
        .route("/api/v1/jobs/{id}/progress", post(jobs::progress))
        .route("/api/v1/jobs/{id}/failure", post(jobs::failure))
        .route("/api/v1/jobs/{id}/relay", post(jobs::relay))
        .layer(axum::extract::DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            JSON_API_TIMEOUT,
        ))
}

fn transfer_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries/{id}/scan", post(libraries::scan))
        .route("/api/v1/jobs/events", get(jobs::events))
        .route("/api/v1/ai/runs/{id}/events", get(crate::ai::run_events))
        .route(
            "/api/v1/jobs/{id}/output",
            get(transfers::output).delete(transfers::delete_output),
        )
        .route("/api/v1/jobs/{id}/source", get(transfers::source))
        .route("/api/v1/worker/transfers/redeem", post(transfers::redeem))
        .route(
            "/api/v1/worker/source-relay/pending",
            get(source_relay::pending),
        )
        .route(
            "/api/v1/worker/source-relay/{request_id}",
            put(source_relay::send),
        )
        .route(
            "/api/v1/worker/source-publications/pending",
            get(source_publication::pending),
        )
        .route(
            "/api/v1/worker/source-publications/{id}/staged",
            get(source_publication::staged),
        )
        .route(
            "/api/v1/worker/source-publications/{id}/authorize",
            get(source_publication::authorize_commit),
        )
        .route(
            "/api/v1/worker/source-publications/{id}/report",
            post(source_publication::report),
        )
        .route(
            "/api/v1/worker/source-publication-inspections/pending",
            get(source_publication::inspection_pending),
        )
        .route(
            "/api/v1/worker/source-publication-inspections/{id}/report",
            post(source_publication::inspection_report),
        )
        .route("/api/v1/jobs/{id}/result", put(transfers::result))
        .route(
            "/api/v1/files/{name}",
            put(libraries::upload_file).delete(libraries::delete_file),
        )
        .route(
            "/api/v1/libraries/{id}/files/{name}",
            post(libraries::upload_to_library),
        )
        .route(
            "/api/v1/local/items/{id}/file",
            get(local::download_resource),
        )
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TRANSFER_TIMEOUT,
        ))
}

fn ai_chat_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/ai/chat", post(crate::ai::chat))
        .route("/api/v1/ai/runs", post(crate::ai::start_run))
        .route(
            "/api/v1/ai/diagnosis/refresh",
            post(crate::ai::refresh_diagnosis),
        )
        .route(
            "/api/v1/ai/conversations",
            get(crate::ai::list_conversations).delete(crate::ai::delete_conversations),
        )
        .route(
            "/api/v1/ai/conversations/{id}",
            get(crate::ai::get_conversation).delete(crate::ai::delete_conversation),
        )
        .route(
            "/api/v1/ai/runs/{id}/actions/{action}",
            post(crate::ai::decide_action),
        )
        .route(
            "/api/v1/ai/runs/{id}",
            axum::routing::delete(crate::ai::cancel_run),
        )
        .layer(axum::extract::DefaultBodyLimit::max(LARGE_JSON_BODY_LIMIT))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            AI_CHAT_TIMEOUT,
        ))
}

pub fn router(state: AppState) -> Router {
    json_routes()
        .merge(ai_chat_routes())
        .merge(transfer_routes())
        .fallback(spa_fallback)
        .layer(axum::middleware::map_response(|response: axum::response::Response| async move {
            let status = response.status();
            if status.is_client_error() && !response.headers().get("content-type").is_some_and(|v| v == "application/json") {
                use axum::response::IntoResponse;
                return (status, axum::Json(serde_json::json!({"error":{"code":"invalid_request"}}))).into_response();
            }
            response
        }))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
            // Do not log query strings: lease credentials belong in headers only.
            tracing::info_span!("http", method=%request.method(), path=%request.uri().path(), request_id=?request.headers().get("x-request-id"))
        }).on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO)))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

async fn spa_fallback(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> axum::response::Response {
    serve_spa(&state.config.static_dir, req).await
}

/// Politique de cache des fichiers de l'interface : revalidation à chaque chargement.
const INTERFACE_CACHE_CONTROL: &str = "no-cache";

/// Sert l'interface (SPA) depuis `static_dir`, ou un 404 JSON pour une route d'API inconnue.
pub(crate) async fn serve_spa(
    static_dir: &std::path::Path,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use tower::ServiceExt;
    let not_found = || {
        (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error":{"code":"invalid_request"}})),
        )
            .into_response()
    };
    if req.uri().path().starts_with("/api/") {
        return not_found();
    }
    let index_file = static_dir.join("index.html");
    if !index_file.is_file() {
        return not_found();
    }
    let serve_service = tower_http::services::ServeDir::new(static_dir)
        .fallback(tower_http::services::ServeFile::new(&index_file));
    match serve_service.oneshot(req).await {
        Ok(res) => {
            let mut res = res.into_response();
            // La page et le module WASM gardent le même nom d'une version à l'autre : le navigateur
            // doit revalider (ETag, Last-Modified) pour ne jamais garder une interface périmée.
            res.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static(INTERFACE_CACHE_CONTROL),
            );
            res
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) const OPENAPI_SPEC: &str = include_str!("../docs/openapi.yaml");

async fn openapi_yaml(_auth: ViewOverviewAuth) -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/yaml; charset=utf-8",
        )],
        OPENAPI_SPEC,
    )
}

async fn health(State(s): State<AppState>) -> AppResult<StatusCode> {
    repository::ping(&s.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}
const SQLITE_BUSY_TIMEOUT_SECS: u64 = 15;
const SQLITE_ACQUIRE_TIMEOUT_SECS: u64 = 15;
const SQLITE_MAX_CONNECTIONS: u32 = 16;

pub(crate) fn client_remote_addr(
    headers: &HeaderMap,
    connect_info: Option<Extension<ConnectInfo<std::net::SocketAddr>>>,
) -> Option<String> {
    if let Some(forwarded) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| raw.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(forwarded.to_owned());
    }
    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(real_ip.to_owned());
    }
    connect_info.map(|Extension(ConnectInfo(addr))| addr.ip().to_string())
}

async fn register(
    State(s): State<AppState>,
    h: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<std::net::SocketAddr>>>,
    axum::Json(r): axum::Json<Register>,
) -> AppResult<axum::Json<Worker>> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    if !crate::worker_settings::is_valid_worker_name(&r.name) {
        return Err(crate::error::AppError::Invalid);
    }
    let remote_addr = client_remote_addr(&h, connect_info);
    let worker = repository::register_with_metadata(
        &s.pool,
        &r.name,
        r.version.as_deref(),
        remote_addr.as_deref(),
        r.telemetry.as_ref(),
    )
    .await?;
    // Une adresse absente ou invalide efface l'annonce : seul le repli par le central reste possible.
    let transfer_url = r
        .transfer_url
        .as_deref()
        .and_then(repository::valid_transfer_url);
    repository::set_transfer_url(&s.pool, worker.id, transfer_url.as_deref()).await?;
    Ok(axum::Json(worker))
}
/// Enrichit un bail (`Claim`) avec les contrats d'opérations, outils requis et réglages nécessaires.
pub(crate) async fn populate_claim_contracts(
    pool: &sqlx::SqlitePool,
    config: &Config,
    catalog_cache: &crate::plugins::CatalogCache,
    worker_id: uuid::Uuid,
    claim: &mut Claim,
) -> AppResult<()> {
    let roots = crate::plugins::catalog_roots(pool, config).await;
    let catalog = crate::plugins::list_catalog(&roots);
    let registry = catalog_cache.operations(&roots).await;
    if let Some(probe_op) =
        registry.primary_probe_for_file(std::path::Path::new(&claim.source_name))
    {
        if let crate::flows::OperationRuntime::Binary(bin) = &probe_op.runtime
            && let Some(tool) = &bin.tool
        {
            let version = catalog_cache.resolve_tool_version(&roots, &tool.id).await;
            claim.probe_tool_id = Some(tool.id.clone());
            claim.probe_version = Some(version);
        }
    } else if registry.all_probes().is_empty()
        && let Some(probe) = crate::plugins::required_probe(&catalog)
    {
        claim.probe_tool_id = Some(probe.tool_id);
        claim.probe_version = Some(probe.version);
    }
    let tool_refs = registry.required_tool_refs_for_flow(claim.frozen_flow.as_ref());
    let mut required_tools = Vec::with_capacity(tool_refs.len());
    for tool in tool_refs {
        let version = catalog_cache.resolve_tool_version(&roots, &tool.id).await;
        required_tools.push(ToolRequirement {
            tool_id: tool.id,
            version,
        });
    }
    claim.required_tools = required_tools;

    let (operations, block_operations) =
        registry.claim_payload_for_flow(claim.frozen_flow.as_ref());
    claim.operations = operations;
    claim.block_operations = block_operations;

    let effective = crate::runtime_settings::load_effective(pool, config).await;
    claim.allow_private_integrations = Some(effective.integration_policy().allow_private_hosts);

    if let Some(ref mut flow) = claim.frozen_flow
        && let Ok(secrets) = crate::secrets::SecretBox::open(&config.config_dir)
    {
        flow.definition =
            crate::flows::decrypt_definition_headers(&secrets, &flow.id, &flow.definition)?;
    }
    if let Some(flow) = claim.frozen_flow.as_ref() {
        claim.plugin_configs =
            crate::flows::load_flow_plugin_configs(pool, &config.config_dir, &registry, Some(flow))
                .await;
        if !claim.plugin_configs.is_empty() {
            let mut media_root = config.input_dir.clone();
            if let Some(source) = repository::job_source(pool, claim.job_id).await? {
                media_root = source.source_root.into();
            } else if let Some(lib_id) = claim.library_id
                && let Ok(Some(local_path)) =
                    repository::library_worker_local_path(pool, lib_id, worker_id).await
            {
                media_root = local_path.into();
            }
            claim.media_root = Some(media_root);
        }
    }
    Ok(())
}

async fn claim(
    State(s): State<AppState>,
    h: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<std::net::SocketAddr>>>,
    axum::Json(r): axum::Json<ClaimRequest>,
) -> AppResult<axum::Json<Option<Claim>>> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let remote_addr = client_remote_addr(&h, connect_info);
    let mut claim_res = repository::claim(&s.pool, r.worker_id).await?;
    if remote_addr.is_some() || r.telemetry.is_some() {
        let _ = repository::touch_worker_telemetry(
            &s.pool,
            r.worker_id,
            remote_addr.as_deref(),
            r.telemetry.as_ref(),
        )
        .await;
    }
    if let Some(ref mut claim) = claim_res {
        populate_claim_contracts(&s.pool, &s.config, &s.catalog_cache, r.worker_id, claim).await?;
        repository::attach_source_ticket(&s.pool, r.worker_id, claim).await?;
        if let Some(worker_key) = crate::auth::bearer_token(&h) {
            crate::flows::seal_claim_secrets_for_worker(claim, worker_key)?;
        }
        s.notify_job_event(
            JobEventType::JobClaimed,
            claim.job_id,
            JobStatus::Running,
            None,
        );
    }
    Ok(axum::Json(claim_res))
}

/// Intervalle entre deux synchronisations automatiques des dépôts de plugins.
const PLUGIN_SYNC_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Intervalle entre deux vérifications des scans planifiés de bibliothèques.
const LIBRARY_SCAN_TICK: Duration = Duration::from_secs(60);

/// Synchronise les dépôts de plugins au démarrage, puis à intervalle régulier.
async fn plugin_sync_loop(state: AppState) {
    loop {
        crate::plugins::sync_enabled_repositories(&state.pool, &state.config).await;
        state.catalog_cache.invalidate().await;
        let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
        let cache = crate::tools_cache::ToolCacheManager::new(&state.config.tools_cache_dir);
        let keep: Vec<(String, String)> = state
            .catalog_cache
            .tools_status(&roots, &cache)
            .await
            .into_iter()
            .map(|t| (t.id, t.version))
            .collect();
        if !keep.is_empty()
            && let Ok(pruned) = cache.prune_unused(&keep)
            && pruned > 0
        {
            tracing::info!(
                pruned,
                "anciennes versions d'outils purgées du cache central"
            );
        }
        tokio::time::sleep(PLUGIN_SYNC_INTERVAL).await;
    }
}

/// Exécute périodiquement les scans de bibliothèques configurés avec un intervalle.
async fn library_scan_loop(state: AppState) {
    loop {
        tokio::time::sleep(LIBRARY_SCAN_TICK).await;
        libraries::run_scheduled_scans(&state).await;
    }
}

/// Ouvre la base SQLite du central et applique les migrations embarquées.
pub async fn connect_database(config: &Config) -> anyhow::Result<SqlitePool> {
    let connect_options = if config.database_url.starts_with("sqlite:") {
        SqliteConnectOptions::from_str(&config.database_url)?
    } else {
        SqliteConnectOptions::new().filename(&config.database_url)
    }
    .create_if_missing(true)
    .journal_mode(SqliteJournalMode::Wal)
    .synchronous(SqliteSynchronous::Normal)
    .foreign_keys(true)
    .busy_timeout(Duration::from_secs(SQLITE_BUSY_TIMEOUT_SECS))
    .pragma("temp_store", "MEMORY")
    .pragma("cache_size", "-16000")
    .pragma("wal_autocheckpoint", "1000");

    let pool = SqlitePoolOptions::new()
        .max_connections(SQLITE_MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(SQLITE_ACQUIRE_TIMEOUT_SECS))
        .connect_with(connect_options)
        .await?;

    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

pub async fn serve(config: Config) -> anyhow::Result<()> {
    let pool = connect_database(&config).await?;
    tokio::fs::create_dir_all(config.data_dir.join("output")).await?;
    tokio::fs::create_dir_all(config.data_dir.join("staging")).await?;
    tokio::fs::create_dir_all(&config.input_dir).await?;
    tokio::fs::create_dir_all(&config.config_dir).await?;
    let addr = config.bind;
    let tls = config.tls.clone();
    let state = AppState::new(pool.clone(), Arc::new(config));
    source_recovery::recover(&state).await?;
    let pending_publications = repository::pending_source_publications(&state.pool).await?;
    if state.config.local_worker {
        tokio::spawn(crate::local_worker::run(state.clone()));
    }
    match crate::secrets::SecretBox::open(&state.config.config_dir) {
        Ok(secrets) => {
            match crate::flows::migrate_flow_secrets(&state.pool, &secrets).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(
                    count,
                    "en-têtes de webhook chiffrés dans les flux existants"
                ),
                Err(error) => {
                    tracing::warn!(error = %error, "migration des en-têtes de webhook impossible")
                }
            }
            if !secrets.retired_key_ids().is_empty() {
                let external = crate::secrets::external_key_file();
                match crate::secret_rotation::reseal_and_retire(
                    &state.pool,
                    &state.config.config_dir,
                    external.as_deref(),
                )
                .await
                {
                    Ok(report) => tracing::info!(
                        plugin_values = report.plugin_values,
                        flow_values = report.flow_values,
                        "rotation de clé achevée au démarrage, anciennes clés retirées"
                    ),
                    Err(error) => tracing::warn!(
                        error = %error,
                        "rechiffrement impossible : les anciennes clés restent dans le trousseau"
                    ),
                }
            }
            match crate::plugins::migrate_legacy_plugin_secrets(&state.pool, &secrets).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(
                    count,
                    "secrets de plugins repris sous les identifiants du catalogue 2.0.0"
                ),
                Err(error) => {
                    tracing::warn!(error = %error, "reprise des secrets de plugins impossible")
                }
            }
        }
        Err(error) => tracing::warn!(error = %error, "clé de chiffrement indisponible"),
    }
    let local_dir = crate::plugins::local_catalog_dir(&state.config);
    match crate::plugins::migrate_legacy_block_kinds(&state.pool, &local_dir).await {
        Ok(0) => {}
        Ok(count) => tracing::info!(
            count,
            "types de blocs repris sous les identifiants du catalogue 2.0.0"
        ),
        Err(error) => tracing::warn!(error = %error, "reprise des types de blocs impossible"),
    }
    let plugin_sync = tokio::spawn(plugin_sync_loop(state.clone()));
    let lib_scans = tokio::spawn(library_scan_loop(state.clone()));
    let ai_diagnosis = tokio::spawn(crate::ai::diagnosis::run_loop(state.clone()));
    let source_recovery = tokio::spawn(source_recovery_remote::recover(
        state.clone(),
        pending_publications,
    ));
    let app = router(state);
    tracing::info!(%addr, "central started");
    if let Some((cert, key)) = tls {
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
        let handle = axum_server::Handle::new();
        let shutdown = handle.clone();
        let signal = tokio::spawn(async move {
            shutdown_signal().await;
            shutdown.graceful_shutdown(Some(Duration::from_secs(15)));
        });
        let result = axum_server::bind_rustls(addr, tls)
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await;
        signal.abort();
        result?;
    } else {
        axum::serve(
            tokio::net::TcpListener::bind(addr).await?,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    }
    plugin_sync.abort();
    lib_scans.abort();
    ai_diagnosis.abort();
    source_recovery.abort();
    pool.close().await;
    Ok(())
}

/// Arrêt demandé : signal du système ou changement de rôle de l'installation.
pub async fn shutdown_signal() {
    tokio::select! {
        () = os_termination_signal() => {},
        () = crate::instance::role_switch_requested() => {},
    }
}

async fn os_termination_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = signal.recv() => {} }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
