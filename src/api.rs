mod accounts;
pub mod auth;
mod flows;
mod installation;
mod jobs;
mod keys;
mod libraries;
mod plugins;
pub(crate) mod session;
mod setup;
mod system;
pub(crate) mod transfers;
mod worker_view;
use crate::{config::Config, domain::*, error::AppResult, repository};
pub use auth::{
    AdminAuth, AnyAuth, ManageAccountsAuth, ViewFlowsAuth, ViewJobsAuth, ViewLibrariesAuth,
    ViewOverviewAuth, ViewPluginsAuth, ViewWorkersAuth,
};
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{str::FromStr, sync::Arc, time::Duration};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub events_tx: tokio::sync::broadcast::Sender<JobEvent>,
}

impl AppState {
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
        let _ = self.events_tx.send(event);
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
        let _ = self.events_tx.send(event);
    }
}

/// Budget des routes JSON, y compris l'empreinte SHA-256 de `POST /api/v1/jobs`.
pub const JSON_API_TIMEOUT: Duration = Duration::from_secs(60);
/// Budget des transferts de fichiers et du flux SSE, déjà borné dans `receive`.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(3600);
pub const JSON_BODY_LIMIT: usize = 16 * 1024;

/// Routes dont la réponse peut durer bien au-delà d'un appel JSON.
pub fn is_long_running_route(path: &str) -> bool {
    if path == "/api/v1/jobs/events" {
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
        .route("/api/v1/config", get(jobs::system_config))
        .route("/api/v1/files", get(jobs::files))
        .route(
            "/api/v1/libraries",
            get(libraries::list).post(libraries::create),
        )
        .route(
            "/api/v1/libraries/{id}",
            get(libraries::get)
                .put(libraries::update)
                .delete(libraries::delete),
        )
        .route("/api/v1/flows", get(flows::list_flows))
        .route("/api/v1/flows/blocks", get(flows::list_blocks))
        .route(
            "/api/v1/flows/{id}",
            get(flows::get_flow)
                .put(flows::save_flow)
                .delete(flows::delete_flow),
        )
        .route("/api/v1/plugins", get(plugins::catalog))
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
            axum::routing::delete(plugins::delete_repository),
        )
        .route(
            "/api/v1/plugins/repositories/{id}/sync",
            post(plugins::sync_repository),
        )
        .route(
            "/api/v1/plugins/{id}/settings",
            get(plugins::get_settings).put(plugins::save_settings),
        )
        .route("/api/v1/tools", get(plugins::tools))
        .route(
            "/api/v1/tools/{id}/distribution",
            get(plugins::tool_distribution),
        )
        .route("/api/v1/tools/{id}/install", post(plugins::install_tool))
        .route(
            "/api/v1/worker/tools/{id}/distribution",
            get(plugins::worker_tool_distribution),
        )
        .route("/api/v1/auth/session", get(session::current))
        .route("/api/v1/auth/login", post(session::login))
        .route("/api/v1/setup/status", get(setup::status))
        .route("/api/v1/setup/central", post(setup::setup_central))
        .route("/api/v1/setup/worker", post(installation::setup_worker))
        .route("/api/v1/workers/enroll", post(installation::enroll))
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
        .route("/api/v1/me", get(session::me))
        .route("/api/v1/account/password", put(session::change_password))
        .route(
            "/api/v1/accounts",
            get(accounts::list).post(accounts::create),
        )
        .route("/api/v1/system/status", get(system::status))
        .route("/api/v1/accounts/{id}", put(accounts::update))
        .route("/api/v1/api-keys", get(keys::list).post(keys::create))
        .route("/api/v1/api-keys/{id}", axum::routing::delete(keys::revoke))
        .route("/api/v1/auth/logout", post(session::logout))
        .route("/api/v1/jobs", get(jobs::list).post(jobs::create))
        .route("/api/v1/jobs/{id}", get(jobs::get_job))
        .route("/api/v1/jobs/{id}/retry", post(jobs::retry))
        .route("/api/v1/workers", get(jobs::workers).post(register))
        .route(
            "/api/v1/workers/{id}",
            axum::routing::delete(jobs::delete_worker),
        )
        .route("/api/v1/claims", post(claim))
        .route("/api/v1/jobs/{id}/heartbeat", post(jobs::heartbeat))
        .route("/api/v1/jobs/{id}/progress", post(jobs::progress))
        .route("/api/v1/jobs/{id}/failure", post(jobs::failure))
        .layer(axum::extract::DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            JSON_API_TIMEOUT,
        ))
}

fn transfer_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/jobs/events", get(jobs::events))
        .route("/api/v1/jobs/{id}/output", get(transfers::output))
        .route("/api/v1/jobs/{id}/source", get(transfers::source))
        .route("/api/v1/jobs/{id}/result", put(transfers::result))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TRANSFER_TIMEOUT,
        ))
}

pub fn router(state: AppState) -> Router {
    json_routes()
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

async fn health(State(s): State<AppState>) -> AppResult<StatusCode> {
    repository::ping(&s.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn register(
    State(s): State<AppState>,
    h: HeaderMap,
    axum::Json(r): axum::Json<Register>,
) -> AppResult<axum::Json<Worker>> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    if !crate::worker_settings::is_valid_worker_name(&r.name) {
        return Err(crate::error::AppError::Invalid);
    }
    Ok(axum::Json(repository::register(&s.pool, &r.name).await?))
}
async fn claim(
    State(s): State<AppState>,
    h: HeaderMap,
    axum::Json(r): axum::Json<ClaimRequest>,
) -> AppResult<axum::Json<Option<Claim>>> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let mut claim_res = repository::claim(&s.pool, r.worker_id).await?;
    if let Some(ref mut claim) = claim_res {
        let roots = crate::plugins::catalog_roots(&s.pool, &s.config).await;
        let catalog = crate::plugins::list_catalog(&roots);
        if let Some(probe) = crate::plugins::required_probe(&catalog) {
            claim.probe_tool_id = Some(probe.tool_id);
            claim.probe_version = Some(probe.version);
        }
        // Worker distant : déchiffrer les en-têtes webhook et joindre les connexions *arr.
        if let Some(ref mut flow) = claim.frozen_flow
            && let Ok(secrets) = crate::secrets::SecretBox::open(&s.config.config_dir)
        {
            flow.definition =
                crate::flows::decrypt_definition_headers(&secrets, &flow.id, &flow.definition)?;
        }
        let arr =
            crate::flows::ArrContext::load(&s.pool, &s.config.config_dir, &s.config.input_dir)
                .await;
        if !arr.connections.is_empty() {
            claim.arr = Some((&arr).into());
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

/// Synchronise les dépôts de plugins au démarrage, puis à intervalle régulier.
async fn plugin_sync_loop(state: AppState) {
    loop {
        crate::plugins::sync_enabled_repositories(&state.pool, &state.config).await;
        tokio::time::sleep(PLUGIN_SYNC_INTERVAL).await;
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
    .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
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
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        events_tx,
    };
    if state.config.local_worker {
        tokio::spawn(crate::local_worker::run(state.clone()));
    }
    match crate::secrets::SecretBox::open(&state.config.config_dir) {
        Ok(secrets) => match crate::flows::migrate_flow_secrets(&state.pool, &secrets).await {
            Ok(0) => {}
            Ok(count) => tracing::info!(
                count,
                "en-têtes de webhook chiffrés dans les flux existants"
            ),
            Err(error) => {
                tracing::warn!(error = %error, "migration des en-têtes de webhook impossible")
            }
        },
        Err(error) => tracing::warn!(error = %error, "clé de chiffrement indisponible"),
    }
    let plugin_sync = tokio::spawn(plugin_sync_loop(state.clone()));
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
            .serve(app.into_make_service())
            .await;
        signal.abort();
        result?;
    } else {
        axum::serve(tokio::net::TcpListener::bind(addr).await?, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }
    plugin_sync.abort();
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
