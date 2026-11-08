use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

fn app() -> anyhow::Result<axum::Router> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "postgresql://unused@localhost/unused".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        admin_key: "test-admin-key-not-a-real-credential".into(),
        worker_key: "test-worker-key-not-a-real-credential".into(),
        max_file_bytes: 1024,
        tls: None,
    };
    let pool = PgPoolOptions::new().connect_lazy(&config.database_url)?;
    Ok(router(AppState {
        pool,
        config: Arc::new(config),
    }))
}

#[tokio::test]
async fn protected_routes_reject_missing_keys_before_accessing_resources() -> anyhow::Result<()> {
    for path in [
        "/api/v1/jobs",
        "/api/v1/files",
        "/api/v1/workers",
        "/api/v1/jobs/00000000-0000-0000-0000-000000000001/output",
        "/api/v1/jobs/00000000-0000-0000-0000-000000000001/source",
    ] {
        let response = app()?
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        assert!(response.headers().contains_key("x-request-id"));
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await?)?;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }
    Ok(())
}

#[tokio::test]
async fn malformed_identifiers_and_unknown_routes_have_safe_json_errors() -> anyhow::Result<()> {
    for (path, status) in [
        ("/api/v1/jobs/invalid", StatusCode::BAD_REQUEST),
        ("/unknown", StatusCode::NOT_FOUND),
    ] {
        let response = app()?
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["content-type"], "application/json");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await?)?;
        assert_eq!(body["error"]["code"], "invalid_request");
    }
    Ok(())
}
