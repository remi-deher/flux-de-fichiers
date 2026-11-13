//! Port HTTP d'un worker : santé, état de l'installation, connexion et vues en lecture seule (ADR 0011).
//!
//! L'interface est la même que celle du central ; seule la source des données change : les vues
//! viennent de routes du central limitées au worker, jamais d'un proxy générique.

use crate::{
    api::{self, session},
    auth,
    central_client::{CentralClient, CentralError},
    config::{self, Config},
    error::{AppError, AppResult},
    plugins::{self, ToolStatusSummary},
    tools_cache::ToolCacheManager,
    web_session,
    worker_accounts::AccountStore,
    worker_sessions::SessionStore,
    worker_view::VerifiedAccount,
};
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
    routing::{get, post},
};
use flux_shared::{InstanceKind, Permission};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Intervalle entre deux copies des comptes depuis le central.
const ACCOUNT_SYNC_INTERVAL: Duration = Duration::from_secs(60);

/// Ce que le worker sait de lui-même ; la boucle de worker met les indicateurs à jour.
pub struct WorkerState {
    pub name: String,
    pub central_url: String,
    registered: AtomicBool,
    central_reachable: AtomicBool,
}

impl WorkerState {
    pub fn new(name: &str, central_url: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_owned(),
            central_url: central_url.to_owned(),
            registered: AtomicBool::new(false),
            central_reachable: AtomicBool::new(false),
        })
    }

    pub fn mark_registered(&self) {
        self.registered.store(true, Ordering::Relaxed);
    }

    pub fn mark_central_reachable(&self, reachable: bool) {
        self.central_reachable.store(reachable, Ordering::Relaxed);
    }
}

/// Tout ce dont l'interface d'un worker a besoin : état, central, comptes copiés et sessions.
pub struct WorkerWeb {
    status: Arc<WorkerState>,
    config: Arc<Config>,
    central: CentralClient,
    accounts: AccountStore,
    sessions: SessionStore,
    tools_cache_dir: PathBuf,
}

impl WorkerWeb {
    pub fn new(
        status: Arc<WorkerState>,
        config: Arc<Config>,
        central: CentralClient,
        tools_cache_dir: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            accounts: AccountStore::load(&config.config_dir),
            sessions: SessionStore::new(config.web_session_ttl_seconds),
            status,
            config,
            central,
            tools_cache_dir,
        })
    }

    /// Copie les comptes du central ; les sessions des comptes disparus ou désactivés sont fermées.
    pub async fn sync_accounts(&self) -> Result<(), CentralError> {
        let snapshot = self.central.accounts().await?;
        match self.accounts.replace(snapshot) {
            Ok(active) => self.sessions.retain_accounts(&active),
            Err(error) => tracing::warn!(error = %error, "copie des comptes impossible"),
        }
        Ok(())
    }

    /// Synchronise les comptes au démarrage puis à intervalle régulier, jusqu'à l'arrêt de la tâche.
    pub async fn sync_accounts_loop(self: Arc<Self>) {
        loop {
            if self.sync_accounts().await.is_err() {
                tracing::debug!("comptes non synchronisés : central indisponible");
            }
            tokio::time::sleep(ACCOUNT_SYNC_INTERVAL).await;
        }
    }

    /// Compte connecté et droit exigé : session mémoire, avec CSRF et origine si la requête modifie.
    fn authorize(
        &self,
        headers: &HeaderMap,
        method: &Method,
        required: Permission,
    ) -> AppResult<VerifiedAccount> {
        let token_hash = web_session::session_token_hash(headers)?;
        let (session, account) = self
            .sessions
            .lookup(&token_hash)
            .ok_or(AppError::Unauthorized)?;
        if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
            web_session::validate_origin(headers, &self.config)?;
            web_session::validate_csrf(headers, &session)?;
        }
        if !account.role.allows(required) {
            return Err(AppError::Forbidden);
        }
        Ok(account)
    }
}

type Web = State<Arc<WorkerWeb>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerStatus {
    instance_kind: InstanceKind,
    name: String,
    central_url: String,
    version: &'static str,
    registered: bool,
    central_reachable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupStatus {
    needs_setup: bool,
    instance_name: String,
    instance_kind: InstanceKind,
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn status(State(web): Web) -> Json<WorkerStatus> {
    let state = &web.status;
    Json(WorkerStatus {
        instance_kind: InstanceKind::Worker,
        name: state.name.clone(),
        central_url: state.central_url.clone(),
        version: env!("CARGO_PKG_VERSION"),
        registered: state.registered.load(Ordering::Relaxed),
        central_reachable: state.central_reachable.load(Ordering::Relaxed),
    })
}

/// Même route que le central : l'interface sait ainsi qu'elle parle à un worker déjà enrôlé.
async fn setup_status(State(web): Web) -> Json<SetupStatus> {
    Json(SetupStatus {
        needs_setup: false,
        instance_name: web.status.name.clone(),
        instance_kind: InstanceKind::Worker,
    })
}

/// Connexion : le central vérifie quand il répond ; la copie locale ne sert que si elle est
/// injoignable (jamais quand elle refuse, pour que révocation et nouveau mot de passe soient immédiats).
async fn login(
    State(web): Web,
    headers: HeaderMap,
    Json(request): Json<session::LoginRequest>,
) -> AppResult<Response> {
    web_session::validate_origin(&headers, &web.config)?;
    let username = request.username.trim().to_owned();
    auth::check_login_allowed(&username)?;
    let account = match web.central.login(&username, &request.password).await {
        Ok(account) => Some(account),
        Err(CentralError::Rejected(StatusCode::TOO_MANY_REQUESTS)) => {
            return Err(AppError::RateLimited);
        }
        Err(CentralError::Rejected(_)) => None,
        Err(CentralError::Unreachable) => web.accounts.verify(&username, request.password).await,
    };
    let Some(account) = account else {
        auth::record_login_failure(&username);
        return Err(AppError::Unauthorized);
    };
    auth::record_login_success(&username);
    let opened = web.sessions.open(&account);
    session::session_response(opened, &account.username, &web.config)
}

async fn current_session(State(web): Web, headers: HeaderMap) -> AppResult<Response> {
    if headers.contains_key(axum::http::header::ORIGIN) {
        web_session::validate_origin(&headers, &web.config)?;
    }
    let token_hash = web_session::session_token_hash(&headers)?;
    let (stored, account) = web
        .sessions
        .lookup(&token_hash)
        .ok_or(AppError::Unauthorized)?;
    let csrf_token = web_session::validate_csrf_cookie(&headers, &stored)?.to_owned();
    Ok(session::no_store(
        axum::response::IntoResponse::into_response(Json(session::SessionResponse {
            csrf_token,
            expires_at: stored.expires_at.to_rfc3339(),
            username: account.username,
        })),
    ))
}

async fn logout(State(web): Web, headers: HeaderMap) -> AppResult<Response> {
    web_session::validate_origin(&headers, &web.config)?;
    let token_hash = web_session::session_token_hash(&headers)?;
    let (stored, _) = web
        .sessions
        .lookup(&token_hash)
        .ok_or(AppError::Unauthorized)?;
    web_session::validate_csrf(&headers, &stored)?;
    web.sessions.close(&token_hash);
    session::cleared_session_response(&web.config)
}

async fn me(State(web): Web, headers: HeaderMap) -> AppResult<Json<session::MeResponse>> {
    let account = web.authorize(&headers, &Method::GET, Permission::ViewOverview)?;
    Ok(Json(session::MeResponse::new(
        "account",
        account.username,
        account.role,
        InstanceKind::Worker,
        Some(web.status.name.clone()),
    )))
}

async fn overview(
    State(web): Web,
    headers: HeaderMap,
) -> AppResult<Json<crate::worker_view::WorkerOverview>> {
    web.authorize(&headers, &Method::GET, Permission::ViewOverview)?;
    Ok(Json(web.central.overview().await?))
}

async fn jobs(State(web): Web, headers: HeaderMap) -> AppResult<Json<serde_json::Value>> {
    web.authorize(&headers, &Method::GET, Permission::ViewJobs)?;
    Ok(Json(web.central.jobs().await?))
}

async fn catalog(
    State(web): Web,
    headers: HeaderMap,
) -> AppResult<Json<Vec<plugins::CatalogEntry>>> {
    web.authorize(&headers, &Method::GET, Permission::ViewPlugins)?;
    Ok(Json(web.central.plugins().await?))
}

/// Outils du catalogue du central, avec l'état d'installation dans le cache local.
async fn tools(State(web): Web, headers: HeaderMap) -> AppResult<Json<Vec<ToolStatusSummary>>> {
    web.authorize(&headers, &Method::GET, Permission::ViewPlugins)?;
    let entries = web.central.plugins().await?;
    let cache = ToolCacheManager::new(&web.tools_cache_dir);
    Ok(Json(plugins::tool_statuses_for_entries(entries, &cache)))
}

async fn interface(State(web): Web, request: Request) -> Response {
    api::serve_spa(&web.config.static_dir, request).await
}

pub fn router(web: Arc<WorkerWeb>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/instance/status", get(status))
        .route("/api/v1/setup/status", get(setup_status))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/session", get(current_session))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/me", get(me))
        .route(crate::worker_view::OVERVIEW_PATH, get(overview))
        .route("/api/v1/jobs", get(jobs))
        .route("/api/v1/plugins", get(catalog))
        .route("/api/v1/tools", get(tools))
        .fallback(interface)
        .with_state(web)
}

/// Sert le port HTTP du worker jusqu'à l'arrêt. Un port occupé n'empêche pas le worker de travailler.
pub async fn serve(web: Arc<WorkerWeb>) {
    let bind = match config::bind_address() {
        Ok(bind) => bind,
        Err(error) => {
            tracing::warn!(error = %error, "adresse d'écoute du worker invalide");
            return;
        }
    };
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(error = %error, %bind, "port HTTP du worker indisponible");
            return;
        }
    };
    tracing::info!(%bind, "worker HTTP started");
    let served = axum::serve(listener, router(web))
        .with_graceful_shutdown(api::shutdown_signal())
        .await;
    if let Err(error) = served {
        tracing::warn!(error = %error, "serveur HTTP du worker arrêté");
    }
}
