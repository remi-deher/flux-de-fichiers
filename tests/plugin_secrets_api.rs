mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
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
const RESPONSE_LIMIT_BYTES: usize = 256 * 1024;
const PLUGIN_ID: &str = "plugin.demo";
const SECRET_VALUE: &str = "SECRET-VALUE-123";
const ICON_FILE: &str = "plugins/demo-icon.svg";
const ICON_URI: &str = "/api/v1/plugins/plugin.demo/icon";
const DEMO_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 4h16v16H4z"/></svg>"#;

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
            {"id":"ffmpeg","type":"app","version":"7.1.0","name":"FFmpeg","manifestPath":"apps/ffmpeg/manifest.json"},
            {"id":"pack.suite","type":"pack","version":"1.0.0","name":"Suite Démo","manifestPath":"packs/suite/manifest.json"}
        ]}"#,
    )?;
    std::fs::write(
        root.join("plugins/demo.json"),
        r#"{"schemaVersion":"1.1.0","id":"plugin.demo","type":"plugin","version":"1.0.0","name":"Démo",
            "description":"Plugin de démonstration","author":"forgearr","license":"GPL-2.0-or-later",
            "icon":"demo-icon.svg",
            "dependencies":[{"id":"ffmpeg","versionRange":"^7.0.0","kind":"app","optional":true}],
            "compatibility":{"appMin":"1.0.0","platforms":["linux/amd64"]},
            "settingsSchema":{"fields":[
                {"name":"url","label":"URL","type":"url","default":"http://defaut"},
                {"name":"apiKey","label":"Clé","type":"secret","required":true}]}}"#,
    )?;
    std::fs::write(
        root.join("plugins/blocks/run.json"),
        r#"{"schemaVersion":"1.1.0","id":"block.demo.run","version":"1.0.0","name":"Démo : exécuter","description":"Exécute la démo","category":"io","operationId":"operation.demo.run"}"#,
    )?;
    std::fs::write(root.join(ICON_FILE), DEMO_ICON)?;
    std::fs::write(
        root.join("apps/ffmpeg/manifest.json"),
        r#"{"schemaVersion":"1.1.0","id":"ffmpeg","type":"app","version":"7.1.0","name":"FFmpeg","description":"Transcodeur","author":"FFmpeg","license":"GPL-2.0-or-later","tool":{"capability":"encode","interface":"cli"}}"#,
    )?;
    std::fs::write(
        root.join("packs/suite/manifest.json"),
        r#"{"schemaVersion":"1.1.0","id":"pack.suite","type":"pack","version":"1.0.0","name":"Suite Démo","description":"Pack de test","author":"forgearr","license":"GPL-2.0-or-later"}"#,
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

async fn send_with_key(
    app: &axum::Router,
    key: &str,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    read_limit: usize,
) -> anyhow::Result<(StatusCode, axum::http::HeaderMap, Vec<u8>)> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {key}"));
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
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), read_limit).await?.to_vec();
    Ok((status, headers, bytes))
}

async fn get_icon(
    app: &axum::Router,
    icon_uri: &str,
) -> anyhow::Result<(StatusCode, axum::http::HeaderMap, Vec<u8>)> {
    send_with_key(
        app,
        TEST_ADMIN_KEY,
        "GET",
        icon_uri,
        None,
        RESPONSE_LIMIT_BYTES,
    )
    .await
}

async fn call_with_key(
    app: &axum::Router,
    key: &str,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let (status, _, bytes) =
        send_with_key(app, key, method, uri, body, RESPONSE_LIMIT_BYTES).await?;
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    Ok((status, value))
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    call_with_key(app, TEST_ADMIN_KEY, method, uri, body).await
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
            &forgearr::plugins::plugin_secret_aad(PLUGIN_ID, "apiKey"),
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
            &forgearr::plugins::plugin_secret_aad(PLUGIN_ID, "apiKey"),
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
    assert_eq!(detail["author"], "forgearr");
    assert_eq!(detail["compatibility"]["appMin"], "1.0.0");
    let blocks = detail["blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("blocks doit être un tableau"))?;
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["kind"], "block.demo.run");
    assert_eq!(blocks[0]["owner"], PLUGIN_ID);

    let (status, _) = call(&h.app, "GET", "/api/v1/plugins/plugin.inconnu", None).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, tool) = call(&h.app, "GET", "/api/v1/tools/ffmpeg", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tool["summary"]["id"], "ffmpeg");
    assert_eq!(tool["usedByPlugins"], serde_json::json!(["plugin.demo"]));

    // Installer un plugin installe ses outils requis ; sans contrat de distribution,
    // l'échec de la dépendance est rapporté sans faire échouer la requête.
    let (status, installed) =
        call(&h.app, "POST", "/api/v1/plugins/plugin.demo/install", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(installed["itemId"], PLUGIN_ID);
    assert_eq!(installed["installedTools"], serde_json::json!([]));
    assert_eq!(installed["failedTools"][0][0], "ffmpeg");
    assert_eq!(installed["missingDependencies"], serde_json::json!([]));
    let (status, _) = call(&h.app, "POST", "/api/v1/plugins/ffmpeg/install", None).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&h.app, "POST", "/api/v1/tools/ffmpeg/install", None).await?;
    assert_ne!(status, StatusCode::OK);

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
            "contentBase64": forgearr_shared::base64::encode(b"1\n00:00:00,000 --> 00:00:01,000\nOK\n")
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["id"], "resource.sous-titres-fr");

    let (status, dl_headers, dl_bytes) = send_with_key(
        &h.app,
        TEST_ADMIN_KEY,
        "GET",
        "/api/v1/local/items/resource.sous-titres-fr/file",
        None,
        64 * 1024,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(dl_bytes, b"1\n00:00:00,000 --> 00:00:01,000\nOK\n".to_vec());
    assert_eq!(
        dl_headers
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        dl_headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        dl_headers
            .get("content-disposition")
            .and_then(|v| v.to_str().ok()),
        Some("attachment; filename=\"fr.srt\"")
    );

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

    let (status, flow_tpl) = call(
        &h.app,
        "POST",
        "/api/v1/local/flow-templates",
        Some(serde_json::json!({
            "name": "Flux HEVC",
            "fileMask": "*.mkv",
            "definition": {"nodes": [], "connections": []}
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(flow_tpl["id"], "flow.template.flux-hevc");

    let (status, pack) = call(
        &h.app,
        "POST",
        "/api/v1/local/packs",
        Some(serde_json::json!({
            "name": "Pack Tag",
            "description": "Suite complète",
            "components": [{"id": "script.tag-perso", "version": "1.0.0", "kind": "script"}]
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pack["id"], "pack.local.pack-tag");

    let (status, pack_detail) =
        call(&h.app, "GET", "/api/v1/packs/pack.local.pack-tag", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pack_detail["id"], "pack.local.pack-tag");

    let (status, snap) = call(&h.app, "GET", "/api/v1/local", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snap["scripts"].as_array().map(Vec::len), Some(1));
    assert_eq!(snap["resources"].as_array().map(Vec::len), Some(1));
    assert_eq!(snap["blockTemplates"].as_array().map(Vec::len), Some(1));
    assert_eq!(snap["flowTemplates"].as_array().map(Vec::len), Some(1));
    assert_eq!(snap["packs"].as_array().map(Vec::len), Some(1));

    let (status, _) = call(
        &h.app,
        "DELETE",
        "/api/v1/local/items/resource.sous-titres-fr",
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(
        &h.app,
        "GET",
        "/api/v1/local/items/resource.sous-titres-fr/file",
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, snap2) = call(&h.app, "GET", "/api/v1/local", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snap2["resources"].as_array().map(Vec::len), Some(0));
    Ok(())
}

#[derive(Clone, Default)]
struct ViewerCookies {
    session: String,
    csrf: String,
}

impl ViewerCookies {
    fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let mut out = Self::default();
        for value in headers.get_all("set-cookie") {
            let text = value.to_str().unwrap_or_default();
            let pair = text.split(';').next().unwrap_or_default();
            if let Some(token) = pair.strip_prefix("flux_session=") {
                out.session = token.to_owned();
            } else if let Some(token) = pair.strip_prefix("flux_csrf=") {
                out.csrf = token.to_owned();
            }
        }
        out
    }
}

async fn call_viewer(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookies: Option<&ViewerCookies>,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, axum::http::HeaderMap, serde_json::Value)> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "localhost:3000")
        .header("origin", "http://localhost:3000");
    if let Some(c) = cookies {
        builder = builder
            .header(
                "cookie",
                format!("flux_session={}; flux_csrf={}", c.session, c.csrf),
            )
            .header("x-csrf-token", c.csrf.clone());
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
async fn test_local_library_limits_conflicts_and_role_permissions() -> anyhow::Result<()> {
    let h = harness().await?;
    let viewer_pass = "un-bon-mot-de-passe";
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/accounts",
        Some(serde_json::json!({
            "username": "viewer-local",
            "password": viewer_pass,
            "role": "viewer"
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    let (status, headers, _) = call_viewer(
        &h.app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({"username": "viewer-local", "password": viewer_pass})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let viewer = ViewerCookies::from_headers(&headers);

    // Un script > 16 Kio (JSON_BODY_LIMIT) doit passer jusqu'à LARGE_JSON_BODY_LIMIT.
    let big_code = format!(
        "# {}\nprintf '{{\"status\":\"ok\"}}'",
        "x".repeat(24 * 1024)
    );
    let (status, created) = call(
        &h.app,
        "POST",
        "/api/v1/local/scripts",
        Some(serde_json::json!({
            "name": "Gros Script",
            "interpreter": "sh",
            "code": big_code
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["id"], "script.gros-script");

    // Créer à nouveau sans id provoque 409 Conflict ; avec id la mise à jour réussit.
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/local/scripts",
        Some(serde_json::json!({
            "name": "Gros Script",
            "interpreter": "sh",
            "code": "echo dup"
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/local/scripts",
        Some(serde_json::json!({
            "id": "script.gros-script",
            "name": "Gros Script",
            "interpreter": "python3",
            "code": "print('{\"status\":\"ok\"}')"
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    // Au-delà de MAX_SCRIPT_BYTES (512 Kio), rejet 413 Payload Too Large.
    let huge_code = "x".repeat(520 * 1024);
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/local/scripts",
        Some(serde_json::json!({
            "name": "Trop Gros Script",
            "interpreter": "sh",
            "code": huge_code
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    // Modèles de blocs/flows et définition de flow > 16 Kio passent aussi.
    let big_nodes: Vec<_> = (0..180)
        .map(|i| serde_json::json!({"id": format!("n{i}"), "type": "file-input", "name": format!("N{i}"), "config": {"pad": "x".repeat(120)}}))
        .collect();
    let big_graph = serde_json::json!({"nodes": big_nodes, "connections": []});
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/local/block-templates",
        Some(serde_json::json!({
            "name": "Grand Modèle Bloc",
            "definition": big_graph
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/local/flow-templates",
        Some(serde_json::json!({
            "name": "Grand Modèle Flow",
            "definition": big_graph
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(
        &h.app,
        "PUT",
        "/api/v1/flows/flow.large.test",
        Some(serde_json::json!({
            "name": "Grand Flow",
            "file_mask": "*.mkv",
            "is_default": false,
            "definition": serde_json::to_string(&big_graph)?
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    // Ressource > 16 Kio (base64 ~ 32 Kio).
    let raw_asset = vec![b'A'; 24 * 1024];
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/local/resources",
        Some(serde_json::json!({
            "name": "Grosse Police",
            "entryType": "resource.font",
            "filename": "font.ttf",
            "contentBase64": forgearr_shared::base64::encode(&raw_asset)
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    // Permissions : Viewer peut lire GET /api/v1/local et télécharger, mais pas modifier.
    let (status, _, _) = call_viewer(&h.app, "GET", "/api/v1/local", Some(&viewer), None).await?;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = call_viewer(
        &h.app,
        "GET",
        "/api/v1/local/items/resource.grosse-police/file",
        Some(&viewer),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    for (method, uri, body) in [
        (
            "POST",
            "/api/v1/local/scripts",
            Some(serde_json::json!({"name":"x","interpreter":"sh","code":"echo"})),
        ),
        (
            "POST",
            "/api/v1/local/block-templates",
            Some(serde_json::json!({"name":"x","definition":{"nodes":[],"connections":[]}})),
        ),
        (
            "POST",
            "/api/v1/local/flow-templates",
            Some(serde_json::json!({"name":"x","definition":{"nodes":[],"connections":[]}})),
        ),
        ("DELETE", "/api/v1/local/items/script.gros-script", None),
    ] {
        let (status, _, _) = call_viewer(&h.app, method, uri, Some(&viewer), body).await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // Une clé Worker n'a ni ViewLibraries ni ViewFlows -> 403 même sur GET /api/v1/local.
    let (status, _) =
        call_with_key(&h.app, common::WORKER_KEY, "GET", "/api/v1/local", None).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Télécharger un script (non ressource) retourne 404.
    let (status, _) = call(
        &h.app,
        "GET",
        "/api/v1/local/items/script.gros-script/file",
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn plugin_connection_test_and_worker_active_tools_work() -> anyhow::Result<()> {
    let h = harness().await?;

    // Worker récupère les couples outil/version actifs pour la purge de cache.
    let (status, tools_json) = call_with_key(
        &h.app,
        common::WORKER_KEY,
        "GET",
        "/api/v1/worker/tools",
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tools_json[0]["tool_id"], "ffmpeg");
    assert_eq!(tools_json[0]["version"], "7.1.0");

    // Démarrer un petit serveur HTTP local pour vérifier le test de connexion d'un plugin API.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let mock = axum::Router::new().route(
        "/",
        axum::routing::get(|headers: axum::http::HeaderMap| async move {
            let auth = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if auth == "Bearer secret-tok" {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, mock).await });

    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/plugins/plugin.demo/test",
        Some(serde_json::json!({
            "url": format!("http://{addr}"),
            "apiKey": "secret-tok"
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["statusCode"], 200);

    let (status, bad_body) = call(
        &h.app,
        "POST",
        "/api/v1/plugins/plugin.demo/test",
        Some(serde_json::json!({
            "url": format!("http://{addr}"),
            "apiKey": "wrong"
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bad_body["ok"], false);
    assert_eq!(bad_body["statusCode"], 401);

    server.abort();
    Ok(())
}

#[tokio::test]
async fn catalog_runtime_readiness_and_dependency_status_reflect_state() -> anyhow::Result<()> {
    let h = harness().await?;

    // Avant configuration, `plugin.demo` requiert `apiKey` -> `needs_config`.
    let (status, catalog) = call(&h.app, "GET", "/api/v1/plugins", None).await?;
    assert_eq!(status, StatusCode::OK);
    let entries = catalog
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("catalog array missing"))?;
    let demo = entries
        .iter()
        .find(|e| e["id"] == PLUGIN_ID)
        .ok_or_else(|| anyhow::anyhow!("plugin.demo missing"))?;
    assert_eq!(demo["runtime"]["readiness"], "needs_config");
    assert_eq!(demo["runtime"]["hasSettings"], true);
    assert_eq!(demo["runtime"]["configured"], false);
    assert_eq!(demo["runtime"]["blocksCount"], 1);

    let pack = entries
        .iter()
        .find(|e| e["id"] == "pack.suite")
        .ok_or_else(|| anyhow::anyhow!("pack.suite missing"))?;
    assert_eq!(pack["runtime"]["componentsCount"], 1);

    // La fiche détaillée renseigne le statut réel de la dépendance `ffmpeg`.
    let (status, detail) = call(&h.app, "GET", "/api/v1/plugins/plugin.demo", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["dependencies"][0]["id"], "ffmpeg");
    assert_eq!(detail["dependencies"][0]["status"], "not_installed");

    // Après enregistrement du champ obligatoire `apiKey`, `readiness` passe à `ready`.
    let (status, _) = call(
        &h.app,
        "PUT",
        "/api/v1/plugins/plugin.demo/settings",
        Some(serde_json::json!({ "apiKey": SECRET_VALUE })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let (status, catalog_after) = call(&h.app, "GET", "/api/v1/plugins", None).await?;
    assert_eq!(status, StatusCode::OK);
    let demo_after = catalog_after
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("catalog array missing"))?
        .iter()
        .find(|e| e["id"] == PLUGIN_ID)
        .ok_or_else(|| anyhow::anyhow!("plugin.demo missing"))?;
    assert_eq!(demo_after["runtime"]["readiness"], "ready");
    assert_eq!(demo_after["runtime"]["configured"], true);

    Ok(())
}

#[tokio::test]
async fn plugin_icon_is_served_as_an_inert_svg_image() -> anyhow::Result<()> {
    let harness = harness().await?;
    let (status, headers, body) = get_icon(&harness.app, ICON_URI).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/svg+xml");
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(body, DEMO_ICON.as_bytes());
    Ok(())
}

#[tokio::test]
async fn a_plugin_named_scripts_keeps_its_own_routes() -> anyhow::Result<()> {
    let harness = harness().await?;
    let index = harness._dirs.0.path().join("index.json");
    let renamed = std::fs::read_to_string(&index)?.replace(PLUGIN_ID, "scripts");
    std::fs::write(&index, renamed)?;
    let (status, _, body) = get_icon(&harness.app, "/api/v1/plugins/scripts/icon").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, DEMO_ICON.as_bytes());
    let (status, _) = call_with_key(
        &harness.app,
        TEST_ADMIN_KEY,
        "GET",
        "/api/v1/plugins/scripts/settings",
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn an_unsafe_icon_is_never_served() -> anyhow::Result<()> {
    let harness = harness().await?;
    let icon = harness._dirs.0.path().join(ICON_FILE);
    std::fs::write(icon, r#"<svg onload="alert(1)"></svg>"#)?;
    let (status, _, _) = get_icon(&harness.app, ICON_URI).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn plugins_without_an_icon_or_unknown_ones_answer_not_found() -> anyhow::Result<()> {
    let harness = harness().await?;
    for uri in [
        "/api/v1/plugins/ffmpeg/icon",
        "/api/v1/plugins/inconnu/icon",
        "/api/v1/plugins/..%2Fsecret/icon",
    ] {
        let (status, _, _) = get_icon(&harness.app, uri).await?;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    Ok(())
}

#[tokio::test]
async fn plugin_icon_requires_authentication() -> anyhow::Result<()> {
    let harness = harness().await?;
    let request = Request::builder().uri(ICON_URI).body(Body::empty())?;
    let response = harness.app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn catalog_flags_only_the_plugins_that_declare_an_icon() -> anyhow::Result<()> {
    let harness = harness().await?;
    let (status, catalog) = call(&harness.app, "GET", "/api/v1/plugins", None).await?;
    assert_eq!(status, StatusCode::OK);
    let entries = catalog
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("catalog array missing"))?;
    let find = |id: &str| entries.iter().find(|entry| entry["id"] == id);
    let demo = find(PLUGIN_ID).ok_or_else(|| anyhow::anyhow!("plugin.demo missing"))?;
    assert_eq!(demo["hasIcon"], true);
    let pack = find("pack.suite").ok_or_else(|| anyhow::anyhow!("pack.suite missing"))?;
    assert!(pack.get("hasIcon").is_none());
    Ok(())
}
