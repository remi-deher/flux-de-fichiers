//! État du système pour la page Paramètres : réservé aux administrateurs (ADR 0011).

use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

const HOST: &str = "localhost:3000";
const ORIGIN: &str = "http://localhost:3000";
const PASSWORD: &str = "un-bon-mot-de-passe";
const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;
const KEY_ID_HEX_LEN: usize = 8;

async fn app() -> anyhow::Result<(axum::Router, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: dir.path().to_path_buf(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    Ok((
        router(AppState {
            pool,
            config: Arc::new(config),
            events_tx,
        }),
        dir,
    ))
}

#[derive(Clone, Default)]
struct Cookies {
    session: String,
    csrf: String,
}

fn cookies_of(headers: &HeaderMap) -> Cookies {
    let mut cookies = Cookies::default();
    for value in headers.get_all("set-cookie") {
        let pair = value
            .to_str()
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default();
        if let Some(token) = pair.strip_prefix("flux_session=") {
            cookies.session = token.to_owned();
        } else if let Some(token) = pair.strip_prefix("flux_csrf=") {
            cookies.csrf = token.to_owned();
        }
    }
    cookies
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookies: Option<&Cookies>,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, HeaderMap, serde_json::Value)> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", HOST)
        .header("origin", ORIGIN);
    if let Some(cookies) = cookies {
        builder = builder
            .header(
                "cookie",
                format!(
                    "flux_session={}; flux_csrf={}",
                    cookies.session, cookies.csrf
                ),
            )
            .header("x-csrf-token", cookies.csrf.clone());
    }
    let request = match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&json)?))?,
        None => builder.body(Body::empty())?,
    };
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
    Ok((
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    ))
}

#[tokio::test]
async fn test_admin_reads_the_system_status() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-systeme", "password": PASSWORD})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let admin = cookies_of(&headers);

    let (status, _, body) = call(&app, "GET", "/api/v1/system/status", Some(&admin), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["keySource"], "local");
    let key_id = body["keyId"].as_str().unwrap_or_default();
    assert_eq!(key_id.len(), KEY_ID_HEX_LEN);
    assert!(key_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Base en mémoire : pas de fichier, donc pas de taille.
    assert!(body["databaseBytes"].is_null());
    Ok(())
}

#[tokio::test]
async fn test_status_is_refused_without_session_and_to_viewers() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (_, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-systeme2", "password": PASSWORD})),
    )
    .await?;
    let admin = cookies_of(&headers);
    let (created, _, _) = call(
        &app,
        "POST",
        "/api/v1/accounts",
        Some(&admin),
        Some(serde_json::json!({"username": "lecteur-systeme", "password": PASSWORD, "role": "viewer"})),
    )
    .await?;
    assert_eq!(created, StatusCode::CREATED);
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({"username": "lecteur-systeme", "password": PASSWORD})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let viewer = cookies_of(&headers);

    let (anonymous, _, _) = call(&app, "GET", "/api/v1/system/status", None, None).await?;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    let (forbidden, _, _) = call(&app, "GET", "/api/v1/system/status", Some(&viewer), None).await?;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    Ok(())
}
