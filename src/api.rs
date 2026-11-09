mod jobs;
pub(crate) mod transfers;
use crate::{auth::authorize, config::Config, domain::*, error::AppResult, repository};
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{sync::Arc, time::Duration};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { axum::Json(serde_json::json!({"service":"flux-de-fichiers", "interface":false})) }))
        .route("/health", get(health))
        .route("/api/v1/files", get(jobs::files))
        .route("/api/v1/jobs", get(jobs::list).post(jobs::create))
        .route("/api/v1/jobs/{id}", get(jobs::get_job))
        .route("/api/v1/jobs/{id}/output", get(transfers::output))
        .route("/api/v1/workers", get(jobs::workers).post(register))
        .route("/api/v1/claims", post(claim))
        .route("/api/v1/jobs/{id}/heartbeat", post(jobs::heartbeat))
        .route("/api/v1/jobs/{id}/failure", post(jobs::failure))
        .route("/api/v1/jobs/{id}/source", get(transfers::source))
        .route("/api/v1/jobs/{id}/result", put(transfers::result))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(3600)))
        .layer(axum::middleware::map_response(|response: axum::response::Response| async move {
            let status = response.status();
            if status.is_client_error() && !response.headers().get("content-type").is_some_and(|v| v == "application/json") {
                use axum::response::IntoResponse;
                return (status, axum::Json(serde_json::json!({"error":{"code":"invalid_request"}}))).into_response();
            }
            response
        }))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
            // Do not log query strings: lease credentials belong in headers only.
            tracing::info_span!("http", method=%request.method(), path=%request.uri().path(), request_id=?request.headers().get("x-request-id"))
        }).on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO)))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

async fn health(State(s): State<AppState>) -> AppResult<StatusCode> {
    sqlx::query("SELECT 1").execute(&s.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn register(
    State(s): State<AppState>,
    h: HeaderMap,
    axum::Json(r): axum::Json<Register>,
) -> AppResult<axum::Json<Worker>> {
    authorize(&h, &s.config.worker_key)?;
    if r.name.is_empty()
        || r.name.len() > 80
        || !r
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err(crate::error::AppError::Invalid);
    }
    Ok(axum::Json(repository::register(&s.pool, &r.name).await?))
}
async fn claim(
    State(s): State<AppState>,
    h: HeaderMap,
    axum::Json(r): axum::Json<ClaimRequest>,
) -> AppResult<axum::Json<Option<Claim>>> {
    authorize(&h, &s.config.worker_key)?;
    Ok(axum::Json(repository::claim(&s.pool, r.worker_id).await?))
}

pub async fn serve(config: Config) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    tokio::fs::create_dir_all(config.data_dir.join("output")).await?;
    tokio::fs::create_dir_all(config.data_dir.join("staging")).await?;
    tokio::fs::create_dir_all(&config.input_dir).await?;
    let addr = config.bind;
    let tls = config.tls.clone();
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
    };
    if state.config.local_worker {
        tokio::spawn(crate::workers::run_local(state.clone()));
    }
    let app = router(state);
    tracing::info!(%addr, "coordinator started");
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
    pool.close().await;
    Ok(())
}

pub async fn shutdown_signal() {
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
