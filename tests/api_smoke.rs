//! Tests d'intégration rapides en Rust pur pour l'API du central.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

async fn test_app() -> anyhow::Result<axum::Router> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 10_000_000,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    std::fs::create_dir_all(&config.input_dir)?;
    let pool = SqlitePoolOptions::new()
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    Ok(router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    }))
}

#[tokio::test]
async fn test_smoke_health_check() -> anyhow::Result<()> {
    let app = test_app().await?;
    let req = Request::builder()
        .uri("/health")
        .method("GET")
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    Ok(())
}

#[tokio::test]
async fn test_smoke_frontend_served() -> anyhow::Result<()> {
    let app = test_app().await?;
    let req = Request::builder()
        .uri("/")
        .method("GET")
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await?;
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("Flux de Fichiers"));
    Ok(())
}

#[tokio::test]
async fn test_smoke_auth_protection() -> anyhow::Result<()> {
    let app = test_app().await?;

    // Sans clé : rejet 401
    let req_unauth = Request::builder()
        .uri("/api/v1/files")
        .method("GET")
        .body(Body::empty())?;
    let resp_unauth = app.clone().oneshot(req_unauth).await?;
    assert_eq!(resp_unauth.status(), StatusCode::UNAUTHORIZED);

    // Avec clé admin valide : 200 OK
    let req_auth = Request::builder()
        .uri("/api/v1/files")
        .method("GET")
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .body(Body::empty())?;
    let resp_auth = app.oneshot(req_auth).await?;
    assert_eq!(resp_auth.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn test_smoke_config_endpoint() -> anyhow::Result<()> {
    let app = test_app().await?;
    let req = Request::builder()
        .uri("/api/v1/config")
        .method("GET")
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert!(json.get("worker_key").is_none());
    assert_eq!(json["max_file_bytes"], 10_000_000);
    Ok(())
}
