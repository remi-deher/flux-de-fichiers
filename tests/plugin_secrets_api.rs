mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
    secrets::SecretBox,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

const TEST_ADMIN_KEY: &str = "fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef";
const BROADCAST_CAPACITY: usize = 128;
const MAX_TEST_FILE_BYTES: u64 = 1024;
const WEB_SESSION_TTL_SECONDS: i64 = 28_800;
const RESPONSE_LIMIT_BYTES: usize = 16 * 1024;
const PLUGIN_ID: &str = "plugin.demo";
const SECRET_VALUE: &str = "SECRET-VALUE-123";

struct Harness {
    app: axum::Router,
    pool: SqlitePool,
    config_dir: std::path::PathBuf,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

fn write_catalog(root: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(root.join("catalogue"))?;
    std::fs::create_dir_all(root.join("plugins"))?;
    std::fs::write(
        root.join("catalogue/index.json"),
        r#"{"schemaVersion":"1.0.0","entries":[{"id":"plugin.demo","type":"plugin","version":"1.0.0","name":"Démo","manifestPath":"plugins/demo.json"}]}"#,
    )?;
    std::fs::write(
        root.join("plugins/demo.json"),
        r#"{"schemaVersion":"1.1.0","id":"plugin.demo","type":"plugin","version":"1.0.0","name":"Démo",
            "settingsSchema":{"fields":[
                {"name":"url","label":"URL","type":"url","default":"http://defaut"},
                {"name":"apiKey","label":"Clé","type":"secret","required":true}]}}"#,
    )?;
    Ok(())
}

async fn harness() -> anyhow::Result<Harness> {
    let catalog_dir = tempfile::tempdir()?;
    let config_dir = tempfile::tempdir()?;
    write_catalog(catalog_dir.path())?;
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: config_dir.path().to_path_buf(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: Some(catalog_dir.path().to_path_buf()),
        static_dir: "frontend/dist".into(),
        max_file_bytes: MAX_TEST_FILE_BYTES,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: WEB_SESSION_TTL_SECONDS,
        tls: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
    let path = config.config_dir.clone();
    let app = router(AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        events_tx,
    });
    Ok(Harness {
        app,
        pool,
        config_dir: path,
        _dirs: (catalog_dir, config_dir),
    })
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"));
    let request = match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&json)?))?,
        None => {
            builder = builder.header("accept", "application/json");
            builder.body(Body::empty())?
        }
    };
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    Ok((status, value))
}

async fn stored(pool: &SqlitePool) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT settings FROM plugin_settings WHERE plugin_id = ?1")
            .bind(PLUGIN_ID)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn test_secret_is_encrypted_at_rest_and_never_returned() -> anyhow::Result<()> {
    let h = harness().await?;
    let uri = format!("/api/v1/plugins/{PLUGIN_ID}/settings");

    let (status, saved) = call(
        &h.app,
        "PUT",
        &uri,
        Some(serde_json::json!({"url": "http://sonarr", "apiKey": SECRET_VALUE})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(saved["settings"].get("apiKey").is_none());
    assert_eq!(saved["settings"]["url"], "http://sonarr");

    let raw = stored(&h.pool).await?;
    assert!(!raw.contains(SECRET_VALUE));
    assert!(raw.contains("enc:v2:"));

    let (status, read) = call(&h.app, "GET", &uri, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(!read.to_string().contains(SECRET_VALUE));
    let fields = read["fields"].as_array().cloned().unwrap_or_default();
    let secret = fields
        .iter()
        .find(|field| field["name"] == "apiKey")
        .ok_or_else(|| anyhow::anyhow!("champ apiKey absent"))?;
    assert_eq!(secret["isSet"], true);
    assert_eq!(secret["value"], "");
    Ok(())
}

#[tokio::test]
async fn test_empty_secret_keeps_the_stored_value_and_a_new_one_replaces_it() -> anyhow::Result<()>
{
    let h = harness().await?;
    let uri = format!("/api/v1/plugins/{PLUGIN_ID}/settings");
    call(
        &h.app,
        "PUT",
        &uri,
        Some(serde_json::json!({"url": "http://a", "apiKey": SECRET_VALUE})),
    )
    .await?;

    call(
        &h.app,
        "PUT",
        &uri,
        Some(serde_json::json!({"url": "http://b", "apiKey": ""})),
    )
    .await?;
    let secrets = SecretBox::open(&h.config_dir)?;
    let value: serde_json::Value = serde_json::from_str(&stored(&h.pool).await?)?;
    assert_eq!(value["url"], "http://b");
    assert_eq!(
        secrets.decrypt(
            value["apiKey"].as_str().unwrap_or_default(),
            &flux_de_fichiers::plugins::plugin_secret_aad(PLUGIN_ID, "apiKey"),
        )?,
        SECRET_VALUE
    );

    call(
        &h.app,
        "PUT",
        &uri,
        Some(serde_json::json!({"apiKey": "AUTRE"})),
    )
    .await?;
    let value: serde_json::Value = serde_json::from_str(&stored(&h.pool).await?)?;
    assert_eq!(
        secrets.decrypt(
            value["apiKey"].as_str().unwrap_or_default(),
            &flux_de_fichiers::plugins::plugin_secret_aad(PLUGIN_ID, "apiKey"),
        )?,
        "AUTRE"
    );
    Ok(())
}

#[tokio::test]
async fn test_legacy_plaintext_secret_is_encrypted_on_first_read() -> anyhow::Result<()> {
    let h = harness().await?;
    sqlx::query("INSERT INTO plugin_settings (plugin_id, settings) VALUES (?1, ?2)")
        .bind(PLUGIN_ID)
        .bind(format!(
            r#"{{"url":"http://vieux","apiKey":"{SECRET_VALUE}"}}"#
        ))
        .execute(&h.pool)
        .await?;

    let (status, read) = call(
        &h.app,
        "GET",
        &format!("/api/v1/plugins/{PLUGIN_ID}/settings"),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(!read.to_string().contains(SECRET_VALUE));
    let raw = stored(&h.pool).await?;
    assert!(!raw.contains(SECRET_VALUE));
    assert!(raw.contains("enc:v2:"));
    Ok(())
}

#[tokio::test]
async fn test_settings_of_unknown_plugins_and_oversized_payloads_are_refused() -> anyhow::Result<()>
{
    let h = harness().await?;
    let (status, _) = call(
        &h.app,
        "PUT",
        "/api/v1/plugins/plugin.inconnu/settings",
        Some(serde_json::json!({"url": "http://x"})),
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(
        &h.app,
        "PUT",
        &format!("/api/v1/plugins/{PLUGIN_ID}/settings"),
        Some(serde_json::json!({"note": "x".repeat(70 * 1024)})),
    )
    .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    let (status, _) = call(
        &h.app,
        "PUT",
        &format!("/api/v1/plugins/{PLUGIN_ID}/settings"),
        Some(serde_json::json!(["pas", "un", "objet"])),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}
