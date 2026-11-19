//! Enrôlement des workers et choix du rôle worker (ADR 0011, phase 2).

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{AppState, router},
    config::Config,
    worker_settings,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

const HOST: &str = "localhost:3000";
const ORIGIN: &str = "http://localhost:3000";
const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;
const GOOD_PASSWORD: &str = "un-bon-mot-de-passe";
const ADMIN: &str = "admin-central";

async fn app_in(config_dir: &Path) -> anyhow::Result<axum::Router> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: config_dir.to_path_buf(),
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
    Ok(router(AppState::new(pool, Arc::new(config))))
}

async fn post(
    app: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    reply(app.clone().oneshot(request).await?).await
}

async fn get(
    app: &axum::Router,
    uri: &str,
    bearer: Option<&str>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let mut builder = Request::builder().uri(uri).header("host", HOST);
    if let Some(key) = bearer {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    reply(app.clone().oneshot(builder.body(Body::empty())?).await?).await
}

async fn reply(
    response: axum::response::Response,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    ))
}

/// Central installé, avec un administrateur, dans un dossier de configuration isolé.
async fn installed_central(dir: &Path) -> anyhow::Result<axum::Router> {
    let app = app_in(dir).await?;
    let (status, _) = post(
        &app,
        "/api/v1/setup/central",
        serde_json::json!({"name": "Central", "username": ADMIN, "password": GOOD_PASSWORD}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    Ok(app)
}

fn enroll_body(password: &str, worker_name: &str) -> serde_json::Value {
    serde_json::json!({"username": ADMIN, "password": password, "worker_name": worker_name})
}

#[tokio::test]
async fn test_enroll_returns_a_worker_key_once() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let central = installed_central(dir.path()).await?;
    let (status, body) = post(
        &central,
        "/api/v1/workers/enroll",
        enroll_body(GOOD_PASSWORD, "nas-1"),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let key = body["key"].as_str().unwrap_or_default().to_owned();
    assert!(key.starts_with("fdf_"));

    let (status, me) = get(&central, "/api/v1/me", Some(&key)).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["role"], "worker");
    assert_eq!(me["name"], "nas-1");

    let (_, keys) = get(&central, "/api/v1/api-keys", Some(&key)).await?;
    assert_eq!(keys["error"]["code"], "forbidden");
    Ok(())
}

#[tokio::test]
async fn test_enroll_refuses_bad_credentials_and_bad_names() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let central = installed_central(dir.path()).await?;
    let (status, _) = post(
        &central,
        "/api/v1/workers/enroll",
        enroll_body("mauvais-mot-de-passe", "nas-1"),
    )
    .await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = post(
        &central,
        "/api/v1/workers/enroll",
        serde_json::json!({"username": "inconnu", "password": GOOD_PASSWORD, "worker_name": "nas-1"}),
    )
    .await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = post(
        &central,
        "/api/v1/workers/enroll",
        enroll_body(GOOD_PASSWORD, "nom invalide"),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn test_instance_status_is_public_and_follows_the_installation() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let fresh = app_in(dir.path()).await?;
    let (status, body) = get(&fresh, "/api/v1/instance/status", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["instanceKind"], "central");
    assert_eq!(body["installed"], false);
    assert!(body["version"].is_string());

    let other = tempfile::tempdir()?;
    let central = installed_central(other.path()).await?;
    let (_, body) = get(&central, "/api/v1/instance/status", None).await?;
    assert_eq!(body["installed"], true);
    Ok(())
}

#[tokio::test]
async fn test_setup_worker_rejects_invalid_input_and_writes_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let installation = app_in(dir.path()).await?;
    for body in [
        serde_json::json!({"central_url": "http://central:3000", "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nom invalide"}),
        serde_json::json!({"central_url": "ftp://central", "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nas-1"}),
        serde_json::json!({"central_url": "http://central:3000/chemin", "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nas-1"}),
    ] {
        let (status, _) = post(&installation, "/api/v1/setup/worker", body).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(worker_settings::load_worker(dir.path())?, None);
    Ok(())
}

#[tokio::test]
async fn test_setup_worker_reports_an_unreachable_central() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let installation = app_in(dir.path()).await?;
    let (status, body) = post(
        &installation,
        "/api/v1/setup/worker",
        serde_json::json!({"central_url": "http://127.0.0.1:9", "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nas-1"}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"]["code"], "central_unavailable");
    assert_eq!(worker_settings::load_worker(dir.path())?, None);
    Ok(())
}

#[tokio::test]
async fn test_installed_central_cannot_become_a_worker() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let central = installed_central(dir.path()).await?;
    let (status, body) = post(
        &central,
        "/api/v1/setup/worker",
        serde_json::json!({"central_url": "http://127.0.0.1:9", "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nas-1"}),
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "already_configured");
    assert_eq!(worker_settings::load_worker(dir.path())?, None);
    Ok(())
}

/// Enrôlement de bout en bout : un vrai central sur un port local, un worker qui s'y enrôle.
#[tokio::test]
async fn test_blank_installation_enrolls_as_worker_against_a_live_central() -> anyhow::Result<()> {
    let central_dir = tempfile::tempdir()?;
    let central = installed_central(central_dir.path()).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let central_url = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, central).await });

    let worker_dir = tempfile::tempdir()?;
    let installation = app_in(worker_dir.path()).await?;
    let (status, _) = post(
        &installation,
        "/api/v1/setup/worker",
        serde_json::json!({"central_url": central_url, "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nas-1"}),
    )
    .await?;
    assert_eq!(status, StatusCode::ACCEPTED);

    let saved = worker_settings::load_worker(worker_dir.path())?;
    assert_eq!(
        saved,
        Some(worker_settings::WorkerSettings {
            name: "nas-1".into(),
            central_url: central_url.clone(),
        })
    );
    let key = worker_settings::load_worker_key(worker_dir.path())?;
    let raw = std::fs::read_to_string(worker_dir.path().join("worker.json"))?;
    assert!(!raw.contains(&key));

    // La clé reçue ouvre bien le protocole de worker du central.
    let client = reqwest::Client::new();
    let me: serde_json::Value = client
        .get(format!("{central_url}/api/v1/me"))
        .bearer_auth(&key)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(me["role"], "worker");

    // Le choix est définitif : un second enrôlement est refusé.
    let (status, _) = post(
        &installation,
        "/api/v1/setup/worker",
        serde_json::json!({"central_url": central_url, "username": ADMIN, "password": GOOD_PASSWORD, "worker_name": "nas-2"}),
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    server.abort();
    Ok(())
}

#[derive(Clone, Default)]
struct AdminCookies {
    session: String,
    csrf: String,
}

impl AdminCookies {
    fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let mut out = Self::default();
        for value in headers.get_all("set-cookie") {
            let text = value.to_str().unwrap_or_default();
            let pair = text.split(';').next().unwrap_or_default();
            if let Some(tok) = pair.strip_prefix("flux_session=") {
                out.session = tok.to_owned();
            } else if let Some(tok) = pair.strip_prefix("flux_csrf=") {
                out.csrf = tok.to_owned();
            }
        }
        out
    }

    fn header(&self) -> String {
        format!("flux_session={}; flux_csrf={}", self.session, self.csrf)
    }
}

async fn installed_central_with_session(
    dir: &Path,
) -> anyhow::Result<(axum::Router, AdminCookies)> {
    let app = app_in(dir).await?;
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/setup/central")
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Central",
            "username": ADMIN,
            "password": GOOD_PASSWORD
        }))?))?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let cookies = AdminCookies::from_headers(response.headers());
    Ok((app, cookies))
}

async fn admin_call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookies: &AdminCookies,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("cookie", cookies.header())
        .header("x-csrf-token", &cookies.csrf);
    let request = match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&json)?))?,
        None => builder.body(Body::empty())?,
    };
    reply(app.clone().oneshot(request).await?).await
}

#[tokio::test]
async fn test_approval_enrollment_window_approval_rejection_and_one_time_key() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let (central, admin) = installed_central_with_session(dir.path()).await?;

    let (secret, secret_hash) = forgearr::enrollment::generate_secret_and_hash();

    // Fenêtre fermée par défaut : toute demande est refusée sans créer d'état.
    let (status, _) = post(
        &central,
        "/api/v1/workers/requests",
        serde_json::json!({
            "workerName": "nas-approve",
            "version": "0.41.0",
            "secretHash": secret_hash,
        }),
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, list) =
        admin_call(&central, "GET", "/api/v1/workers/requests", &admin, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["window"]["open"], false);
    assert_eq!(list["requests"].as_array().map(Vec::len), Some(0));

    // L'administrateur ouvre la fenêtre de 15 minutes.
    let (status, win) = admin_call(
        &central,
        "POST",
        "/api/v1/workers/enrollment-window",
        &admin,
        Some(serde_json::json!({"open": true})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(win["open"], true);
    assert!(win["expiresAt"].is_string());

    // Le worker dépose une demande (avec X-Forwarded-For pour vérifier l'adresse remontée).
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/workers/requests")
        .header("host", HOST)
        .header("x-forwarded-for", "192.168.1.51")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "workerName": "nas-approve",
            "version": "0.41.0",
            "secretHash": secret_hash,
        }))?))?;
    let (status, created) = reply(central.clone().oneshot(req).await?).await?;
    assert_eq!(status, StatusCode::CREATED);
    let req_id = created["id"].as_str().unwrap_or_default().to_owned();
    assert_eq!(created["status"], "pending");

    // La demande est visible côté admin avec nom, adresse et version.
    let (status, list) =
        admin_call(&central, "GET", "/api/v1/workers/requests", &admin, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["requests"][0]["workerName"], "nas-approve");
    assert_eq!(list["requests"][0]["sourceAddr"], "192.168.1.51");
    assert_eq!(list["requests"][0]["workerVersion"], "0.41.0");

    // Mauvais secret refusé ; bon secret en attente.
    let bad_poll = Request::builder()
        .uri(format!("/api/v1/workers/requests/{req_id}"))
        .header("host", HOST)
        .header("x-enrollment-secret", "mauvais-secret")
        .body(Body::empty())?;
    let (status, _) = reply(central.clone().oneshot(bad_poll).await?).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let poll_req = || -> anyhow::Result<Request<Body>> {
        Ok(Request::builder()
            .uri(format!("/api/v1/workers/requests/{req_id}"))
            .header("host", HOST)
            .header("x-enrollment-secret", &secret)
            .body(Body::empty())?)
    };
    let (status, polled) = reply(central.clone().oneshot(poll_req()?).await?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(polled["status"], "pending");
    assert!(polled["key"].is_null());

    // L'admin approuve la demande ; le worker retire la clé une seule fois.
    let (status, approved) = admin_call(
        &central,
        "POST",
        &format!("/api/v1/workers/requests/{req_id}/approve"),
        &admin,
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["status"], "approved");

    let (status, polled) = reply(central.clone().oneshot(poll_req()?).await?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(polled["status"], "approved");
    let key = polled["key"].as_str().unwrap_or_default().to_owned();
    assert!(key.starts_with("fdf_"));

    // Second retrait impossible : la demande est consommée.
    let (status, _) = reply(central.clone().oneshot(poll_req()?).await?).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // La clé remise authentifie bien le worker.
    let (status, me) = get(&central, "/api/v1/me", Some(&key)).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["role"], "worker");
    assert_eq!(me["name"], "nas-approve");

    // Cas du refus par l'administrateur.
    let (rej_secret, rej_hash) = forgearr::enrollment::generate_secret_and_hash();
    let req2 = Request::builder()
        .method("POST")
        .uri("/api/v1/workers/requests")
        .header("host", HOST)
        .header("x-forwarded-for", "192.168.1.52")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "workerName": "nas-reject",
            "secretHash": rej_hash,
        }))?))?;
    let (status, created2) = reply(central.clone().oneshot(req2).await?).await?;
    assert_eq!(status, StatusCode::CREATED);
    let req2_id = created2["id"].as_str().unwrap_or_default().to_owned();

    let (status, rejected) = admin_call(
        &central,
        "POST",
        &format!("/api/v1/workers/requests/{req2_id}/reject"),
        &admin,
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");

    let poll2 = Request::builder()
        .uri(format!("/api/v1/workers/requests/{req2_id}"))
        .header("host", HOST)
        .header("x-enrollment-secret", &rej_secret)
        .body(Body::empty())?;
    let (status, body2) = reply(central.clone().oneshot(poll2).await?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body2["status"], "rejected");
    assert!(body2["key"].is_null());

    // Fermeture manuelle de la fenêtre.
    let (status, win_closed) = admin_call(
        &central,
        "POST",
        "/api/v1/workers/enrollment-window",
        &admin,
        Some(serde_json::json!({"open": false})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(win_closed["open"], false);
    Ok(())
}

#[tokio::test]
async fn test_blank_installation_enrolls_via_approval_against_live_central() -> anyhow::Result<()> {
    let central_dir = tempfile::tempdir()?;
    let (central, admin) = installed_central_with_session(central_dir.path()).await?;

    let (status, _) = admin_call(
        &central,
        "POST",
        "/api/v1/workers/enrollment-window",
        &admin,
        Some(serde_json::json!({"open": true})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let central_url = format!("http://{}", listener.local_addr()?);
    let central_clone = central.clone();
    let server = tokio::spawn(async move { axum::serve(listener, central_clone).await });

    let worker_dir = tempfile::tempdir()?;
    let installation = app_in(worker_dir.path()).await?;
    let (status, started) = post(
        &installation,
        "/api/v1/setup/worker-request",
        serde_json::json!({"centralUrl": central_url, "workerName": "nas-auto"}),
    )
    .await?;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(started["state"], "pending");

    let (status, list) =
        admin_call(&central, "GET", "/api/v1/workers/requests", &admin, None).await?;
    assert_eq!(status, StatusCode::OK);
    let req_id = list["requests"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    let (status, _) = admin_call(
        &central,
        "POST",
        &format!("/api/v1/workers/requests/{req_id}/approve"),
        &admin,
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    // Attendre que la tâche de polling de l'installation vierge enregistre le worker.
    let mut approved = false;
    for _ in 0..30 {
        if worker_settings::load_worker(worker_dir.path())?.is_some() {
            approved = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(approved, "worker settings should have been persisted");
    let saved = worker_settings::load_worker(worker_dir.path())?;
    assert_eq!(
        saved,
        Some(worker_settings::WorkerSettings {
            name: "nas-auto".into(),
            central_url,
        })
    );
    let key = worker_settings::load_worker_key(worker_dir.path())?;
    assert!(key.starts_with("fdf_"));
    server.abort();
    Ok(())
}
