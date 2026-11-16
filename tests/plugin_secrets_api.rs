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
    std::fs::create_dir_all(root.join("plugins/blocks"))?;
    std::fs::create_dir_all(root.join("apps/ffmpeg"))?;
    std::fs::create_dir_all(root.join("packs/suite"))?;
    std::fs::write(
        root.join("index.json"),
        r#"{"schemaVersion":"1.0.0","entries":[
            {"id":"plugin.demo","type":"plugin","version":"1.0.0","name":"Démo","manifestPath":"plugins/demo.json"},
            {"id":"block.demo.run","type":"block","owner":"plugin.demo","version":"1.0.0","name":"Démo : exécuter","manifestPath":"plugins/blocks/run.json"},
            {"id":"app.ffmpeg","type":"app","version":"7.1.0","name":"FFmpeg","manifestPath":"apps/ffmpeg/manifest.json"},
            {"id":"pack.suite","type":"pack","version":"1.0.0","name":"Suite Démo","manifestPath":"packs/suite/manifest.json"}
        ]}"#,
    )?;
    std::fs::write(
        root.join("plugins/demo.json"),
        r#"{"schemaVersion":"1.1.0","id":"plugin.demo","type":"plugin","version":"1.0.0","name":"Démo",
            "description":"Plugin de démonstration","author":"flux-de-fichiers","license":"GPL-2.0-or-later",
            "dependencies":[{"id":"app.ffmpeg","versionRange":"^7.0.0","kind":"app","optional":true}],
            "compatibility":{"appMin":"1.0.0","platforms":["linux/amd64"]},
            "settingsSchema":{"fields":[
                {"name":"url","label":"URL","type":"url","default":"http://defaut"},
                {"name":"apiKey","label":"Clé","type":"secret","required":true}]}}"#,
    )?;
    std::fs::write(
        root.join("plugins/blocks/run.json"),
        r#"{"schemaVersion":"1.1.0","id":"block.demo.run","version":"1.0.0","name":"Démo : exécuter","description":"Exécute la démo","category":"io","operationId":"operation.demo.run"}"#,
    )?;
    std::fs::write(
        root.join("apps/ffmpeg/manifest.json"),
        r#"{"schemaVersion":"1.1.0","id":"app.ffmpeg","type":"app","version":"7.1.0","name":"FFmpeg","description":"Transcodeur","author":"FFmpeg","license":"GPL-2.0-or-later","tool":{"capability":"encode","interface":"cli"}}"#,
    )?;
    std::fs::write(
        root.join("packs/suite/manifest.json"),
        r#"{"schemaVersion":"1.1.0","id":"pack.suite","type":"pack","version":"1.0.0","name":"Suite Démo","description":"Pack de test","author":"flux-de-fichiers","license":"GPL-2.0-or-later"}"#,
    )?;
    std::fs::write(
        root.join("packs/suite/installation.json"),
        r#"{"schemaVersion":"1.0.0","packId":"pack.suite","packVersion":"1.0.0","kind":"declarative-bundle","components":[{"id":"plugin.demo","version":"1.0.0","kind":"plugin"}]}"#,
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
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    let path = config.config_dir.clone();
    let app = router(AppState::new(pool.clone(), Arc::new(config)));
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

#[tokio::test]
async fn test_plugin_detail_exposes_owned_blocks_and_metadata() -> anyhow::Result<()> {
    let h = harness().await?;
    let (status, detail) =
        call(&h.app, "GET", &format!("/api/v1/plugins/{PLUGIN_ID}"), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], PLUGIN_ID);
    assert_eq!(detail["hasSettings"], true);
    assert_eq!(detail["author"], "flux-de-fichiers");
    assert_eq!(detail["compatibility"]["appMin"], "1.0.0");
    let blocks = detail["blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("blocks doit être un tableau"))?;
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["kind"], "block.demo.run");
    assert_eq!(blocks[0]["owner"], PLUGIN_ID);

    let (status, _) = call(&h.app, "GET", "/api/v1/plugins/plugin.inconnu", None).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, tool) = call(&h.app, "GET", "/api/v1/tools/app.ffmpeg", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tool["summary"]["id"], "app.ffmpeg");
    assert_eq!(tool["usedByPlugins"], serde_json::json!(["plugin.demo"]));

    let (status, pack) = call(&h.app, "GET", "/api/v1/packs/pack.suite", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pack["id"], "pack.suite");
    assert_eq!(pack["description"], "Pack de test");
    assert_eq!(pack["installation"]["components"][0]["id"], "plugin.demo");
    Ok(())
}

#[tokio::test]
async fn test_local_library_crud_without_github() -> anyhow::Result<()> {
    let h = harness().await?;
    let (status, script) = call(
        &h.app,
        "POST",
        "/api/v1/local/scripts",
        Some(serde_json::json!({
            "name": "Tag perso",
            "interpreter": "sh",
            "code": "cat >/dev/null\nprintf '{\"status\":\"ok\"}'"
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(script["id"], "script.tag-perso");

    let (status, res) = call(
        &h.app,
        "POST",
        "/api/v1/local/resources",
        Some(serde_json::json!({
            "name": "Sous-titres FR",
            "entryType": "resource.subtitle",
            "filename": "fr.srt",
            "contentBase64": flux_shared::base64::encode(b"1\n00:00:00,000 --> 00:00:01,000\nOK\n")
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["id"], "resource.sous-titres-fr");

    let (status, tpl) = call(
        &h.app,
        "POST",
        "/api/v1/local/block-templates",
        Some(serde_json::json!({
            "name": "Bloc Remux",
            "definition": {"nodes": [], "connections": []}
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tpl["id"], "template.block.bloc-remux");

    let (status, snap) = call(&h.app, "GET", "/api/v1/local", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snap["scripts"].as_array().map(Vec::len), Some(1));
    assert_eq!(snap["resources"].as_array().map(Vec::len), Some(1));
    assert_eq!(snap["blockTemplates"].as_array().map(Vec::len), Some(1));

    let (status, _) = call(
        &h.app,
        "DELETE",
        "/api/v1/local/items/resource.sous-titres-fr",
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let (status, snap2) = call(&h.app, "GET", "/api/v1/local", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snap2["resources"].as_array().map(Vec::len), Some(0));
    Ok(())
}
