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
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
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

async fn json_of(response: axum::response::Response) -> anyhow::Result<serde_json::Value> {
    let bytes = to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn admin(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<axum::response::Response> {
    let mut builder = Request::builder().method(method).uri(uri).header(
        "authorization",
        "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
    );
    let request = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        builder.body(Body::from(serde_json::to_vec(&body)?))?
    } else {
        builder.body(Body::empty())?
    };
    Ok(app.clone().oneshot(request).await?)
}

async fn register_worker(app: &axum::Router, name: &str) -> anyhow::Result<String> {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/workers")
        .header(
            "authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"name":"{name}"}}"#)))?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await?;
    Ok(body["id"].as_str().unwrap_or_default().to_owned())
}

async fn save_flow(app: &axum::Router, id: &str, name: &str) -> anyhow::Result<()> {
    let response = admin(
        app,
        "PUT",
        &format!("/api/v1/flows/{id}"),
        Some(serde_json::json!({
            "name": name,
            "description": "Flux de la bibliothèque",
            "file_mask": "*.mkv",
            "is_default": false,
            "definition": "{\"nodes\":[],\"connections\":[]}"
        })),
    )
    .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn libraries_round_trip_flow_and_worker_paths() -> anyhow::Result<()> {
    let app = test_app().await?;
    let listed = admin(&app, "GET", "/api/v1/libraries", None).await?;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(json_of(listed).await?, serde_json::json!([]));

    let missing = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({
            "name": "Films",
            "flow_id": "flow.absent",
            "workers": []
        })),
    )
    .await?;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    let relative = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({
            "name": "Films",
            "workers": [{
                "worker_id": "00000000-0000-0000-0000-000000000001",
                "local_path": "media",
                "access": "read"
            }]
        })),
    )
    .await?;
    assert_eq!(relative.status(), StatusCode::BAD_REQUEST);

    save_flow(&app, "flow.films", "Remux films").await?;
    let worker_a = register_worker(&app, "nas-a").await?;
    let worker_b = register_worker(&app, "nas-b").await?;

    let created = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({
            "name": "  Films  ",
            "flow_id": "flow.films",
            "workers": [
                {
                    "worker_id": worker_a,
                    "local_path": "/mnt/films/",
                    "access": "read_write"
                },
                {
                    "worker_id": worker_b,
                    "local_path": "/data/films",
                    "access": "read"
                }
            ]
        })),
    )
    .await?;
    assert_eq!(created.status(), StatusCode::CREATED);
    let library = json_of(created).await?;
    assert_eq!(library["name"], "Films");
    assert_eq!(library["flow_id"], "flow.films");
    assert_eq!(library["workers"].as_array().map(Vec::len), Some(2));
    let id = library["id"].as_str().unwrap_or_default().to_owned();

    let duplicate = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({"name": "Films", "workers": []})),
    )
    .await?;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let updated = admin(
        &app,
        "PUT",
        &format!("/api/v1/libraries/{id}"),
        Some(serde_json::json!({
            "name": "Films",
            "flow_id": "",
            "workers": [{
                "worker_id": worker_a,
                "local_path": "/mnt/films",
                "access": "read"
            }]
        })),
    )
    .await?;
    assert_eq!(updated.status(), StatusCode::OK);
    let library = json_of(updated).await?;
    assert!(library["flow_id"].is_null());
    assert_eq!(library["workers"].as_array().map(Vec::len), Some(1));
    assert_eq!(library["workers"][0]["local_path"], "/mnt/films");
    assert_eq!(library["workers"][0]["access"], "read");

    let twice = admin(
        &app,
        "PUT",
        &format!("/api/v1/libraries/{id}"),
        Some(serde_json::json!({
            "name": "Films",
            "workers": [
                {
                    "worker_id": worker_a,
                    "local_path": "/mnt/a",
                    "access": "read"
                },
                {
                    "worker_id": worker_a,
                    "local_path": "/mnt/b",
                    "access": "read_write"
                }
            ]
        })),
    )
    .await?;
    assert_eq!(twice.status(), StatusCode::BAD_REQUEST);

    save_flow(&app, "flow.other", "Autre flux").await?;
    let reattached = admin(
        &app,
        "PUT",
        &format!("/api/v1/libraries/{id}"),
        Some(serde_json::json!({
            "name": "Films",
            "flow_id": "flow.other",
            "workers": [{
                "worker_id": worker_b,
                "local_path": "/data/films",
                "access": "read_write"
            }]
        })),
    )
    .await?;
    assert_eq!(reattached.status(), StatusCode::OK);

    let removed_flow = admin(&app, "DELETE", "/api/v1/flows/flow.other", None).await?;
    assert_eq!(removed_flow.status(), StatusCode::NO_CONTENT);
    let after_flow = admin(&app, "GET", &format!("/api/v1/libraries/{id}"), None).await?;
    let library = json_of(after_flow).await?;
    assert!(library["flow_id"].is_null());
    assert_eq!(library["workers"][0]["worker_id"], worker_b);

    let removed_worker =
        admin(&app, "DELETE", &format!("/api/v1/workers/{worker_b}"), None).await?;
    assert_eq!(removed_worker.status(), StatusCode::NO_CONTENT);
    let after_worker = admin(&app, "GET", &format!("/api/v1/libraries/{id}"), None).await?;
    let library = json_of(after_worker).await?;
    assert_eq!(library["workers"].as_array().map(Vec::len), Some(0));

    let removed = admin(&app, "DELETE", &format!("/api/v1/libraries/{id}"), None).await?;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let gone = admin(&app, "GET", &format!("/api/v1/libraries/{id}"), None).await?;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    Ok(())
}
