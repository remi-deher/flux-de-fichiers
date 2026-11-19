//! État du système pour la page Paramètres : réservé aux administrateurs (ADR 0011).

use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use forgearr::{
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
const DIAGNOSTIC_CHECK_COUNT: usize = 6;

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
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok((router(AppState::new(pool, Arc::new(config))), dir))
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
    assert_eq!(body["version"], forgearr::version::app_version());
    assert_eq!(body["buildVersion"], forgearr::version::BUILD_VERSION);
    assert_eq!(body["keySource"], "local");
    let key_id = body["keyId"].as_str().unwrap_or_default();
    assert_eq!(key_id.len(), KEY_ID_HEX_LEN);
    assert!(key_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Base en mémoire : pas de fichier, donc pas de taille.
    assert!(body["databaseBytes"].is_null());
    assert_eq!(body["network"]["bind"], "127.0.0.1:3000");
    assert_eq!(body["network"]["tls"], false);
    assert!(body["network"]["webOrigin"].is_null());
    assert_eq!(body["network"]["sessionTtlSeconds"], 28_800);
    assert_eq!(body["storage"]["maxFileBytes"], 1024);
    assert_eq!(body["storage"]["inputDir"], "var/input");
    assert!(body["storage"]["pluginsDir"].is_null());
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

#[tokio::test]
async fn test_hardware_is_readable_by_a_viewer_and_refused_without_session() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (_, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-materiel", "password": PASSWORD})),
    )
    .await?;
    let admin = cookies_of(&headers);
    let (created, _, _) = call(
        &app,
        "POST",
        "/api/v1/accounts",
        Some(&admin),
        Some(serde_json::json!({"username": "lecteur-materiel", "password": PASSWORD, "role": "viewer"})),
    )
    .await?;
    assert_eq!(created, StatusCode::CREATED);
    let (_, headers, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({"username": "lecteur-materiel", "password": PASSWORD})),
    )
    .await?;
    let viewer = cookies_of(&headers);

    let (status, _, body) =
        call(&app, "GET", "/api/v1/system/hardware", Some(&viewer), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(body["logicalCores"].as_u64().unwrap_or(0) >= 1);
    let cpu = body["cpuUsagePercent"].as_f64().unwrap_or(-1.0);
    assert!((0.0..=100.0).contains(&cpu));
    assert!(body["gpuDevices"].is_array());
    let (anonymous, _, _) = call(&app, "GET", "/api/v1/system/hardware", None, None).await?;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn test_diagnostics_are_admin_only_and_list_every_check() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (_, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(
            serde_json::json!({"name": "Central", "username": "admin-diag", "password": PASSWORD}),
        ),
    )
    .await?;
    let admin = cookies_of(&headers);
    let (created, _, _) = call(
        &app,
        "POST",
        "/api/v1/accounts",
        Some(&admin),
        Some(
            serde_json::json!({"username": "lecteur-diag", "password": PASSWORD, "role": "viewer"}),
        ),
    )
    .await?;
    assert_eq!(created, StatusCode::CREATED);
    let (_, headers, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({"username": "lecteur-diag", "password": PASSWORD})),
    )
    .await?;
    let viewer = cookies_of(&headers);
    let request = serde_json::json!({"browserOrigin": ORIGIN});

    let (status, _, body) = call(
        &app,
        "POST",
        "/api/v1/system/diagnostics",
        Some(&admin),
        Some(request.clone()),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let checks = body["checks"].as_array().cloned().unwrap_or_default();
    assert_eq!(checks.len(), DIAGNOSTIC_CHECK_COUNT);
    assert!(checks.iter().all(|check| {
        ["ok", "warning", "error"].contains(&check["status"].as_str().unwrap_or_default())
    }));
    let smtp = checks.iter().find(|check| check["id"] == "smtp");
    assert_eq!(
        smtp.map(|check| check["status"].clone()),
        Some("warning".into())
    );

    let (forbidden, _, _) = call(
        &app,
        "POST",
        "/api/v1/system/diagnostics",
        Some(&viewer),
        Some(request.clone()),
    )
    .await?;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let (anonymous, _, _) = call(
        &app,
        "POST",
        "/api/v1/system/diagnostics",
        None,
        Some(request),
    )
    .await?;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn test_updates_check_is_disabled_by_default_and_can_be_toggled() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-upd", "password": PASSWORD})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let admin = cookies_of(&headers);

    let (status, _, body) = call(&app, "GET", "/api/v1/system/updates", Some(&admin), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], false);
    assert_eq!(body["updateAvailable"], false);

    let (status, _, updated) = call(
        &app,
        "PUT",
        "/api/v1/system/settings",
        Some(&admin),
        Some(serde_json::json!({
            "webOrigin": null,
            "sessionTtlSeconds": 28800,
            "smtpEnabled": false,
            "smtpHost": null,
            "smtpPort": null,
            "smtpTls": "starttls",
            "smtpUsername": null,
            "smtpPassword": null,
            "smtpFrom": null,
            "allowPrivatePluginHosts": false,
            "blockPrivateIntegrationHosts": false,
            "checkUpdatesEnabled": true
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["checkUpdatesEnabled"], true);
    Ok(())
}

#[tokio::test]
async fn test_admin_rotates_the_key_online_and_a_second_rotation_waits() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-rot", "password": PASSWORD})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let admin = cookies_of(&headers);
    let (_, _, before) = call(&app, "GET", "/api/v1/system/status", Some(&admin), None).await?;
    let old_key_id = before["keyId"].as_str().unwrap_or_default().to_owned();
    assert_eq!(before["retiredKeyIds"], serde_json::json!([]));

    let rotate = "/api/v1/system/secret-key/rotate";
    let (status, _, _) = call(&app, "POST", rotate, None, None).await?;
    assert_ne!(status, StatusCode::OK);

    let (status, _, after) = call(&app, "POST", rotate, Some(&admin), None).await?;
    assert_eq!(status, StatusCode::OK);
    let new_key_id = after["keyId"].as_str().unwrap_or_default();
    assert_eq!(new_key_id.len(), KEY_ID_HEX_LEN);
    assert_ne!(new_key_id, old_key_id);
    // L'ancienne clé reste dans le trousseau pendant le délai de grâce.
    assert_eq!(after["retiredKeyIds"], serde_json::json!([old_key_id]));

    let (status, _, _) = call(&app, "POST", rotate, Some(&admin), None).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    Ok(())
}

#[tokio::test]
async fn test_admin_exports_a_recovery_kit_and_restores_it() -> anyhow::Result<()> {
    const KIT_PASSPHRASE: &str = "phrase de passe du kit";
    let (app, _dir) = app().await?;
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-kit", "password": PASSWORD})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let admin = cookies_of(&headers);
    let (_, _, before) = call(&app, "GET", "/api/v1/system/status", Some(&admin), None).await?;
    assert_eq!(before["recoveryKitKeyId"], serde_json::Value::Null);

    let export = "/api/v1/system/secret-key/recovery-kit";
    let (status, _, _) = call(
        &app,
        "POST",
        export,
        Some(&admin),
        Some(serde_json::json!({"passphrase": "courte"})),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, kit) = call(
        &app,
        "POST",
        export,
        Some(&admin),
        Some(serde_json::json!({"passphrase": KIT_PASSPHRASE})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(kit["format"], "flux-recovery-kit");
    assert_eq!(kit["keyId"], before["keyId"]);
    let (_, _, after) = call(&app, "GET", "/api/v1/system/status", Some(&admin), None).await?;
    assert_eq!(after["recoveryKitKeyId"], before["keyId"]);

    let restore = "/api/v1/system/secret-key/restore";
    let (status, _, _) = call(
        &app,
        "POST",
        restore,
        Some(&admin),
        Some(serde_json::json!({"kit": kit, "passphrase": "une autre phrase longue"})),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, restored) = call(
        &app,
        "POST",
        restore,
        Some(&admin),
        Some(serde_json::json!({"kit": kit, "passphrase": KIT_PASSPHRASE})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["keyId"], before["keyId"]);
    assert_eq!(restored["retiredKeyIds"], serde_json::json!([]));
    Ok(())
}

#[tokio::test]
async fn timezone_can_be_read_and_updated_by_admin() -> anyhow::Result<()> {
    let (app, _dir) = app().await?;
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Central", "username": "admin-tz", "password": PASSWORD})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let admin = cookies_of(&headers);

    let (status, _, initial) =
        call(&app, "GET", "/api/v1/system/timezone", Some(&admin), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(initial["timezone"].as_str().is_some());
    assert!(initial["utcOffset"].as_str().is_some());

    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/v1/system/timezone",
        Some(&admin),
        Some(serde_json::json!({"timezone": "Invalid/Timezone"})),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, updated) = call(
        &app,
        "PUT",
        "/api/v1/system/timezone",
        Some(&admin),
        Some(serde_json::json!({"timezone": "Europe/Paris"})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["timezone"], "Europe/Paris");

    let (status, _, after) =
        call(&app, "GET", "/api/v1/system/timezone", Some(&admin), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["timezone"], "Europe/Paris");
    Ok(())
}
