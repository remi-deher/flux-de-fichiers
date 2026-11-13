//! Interface d'un worker : connexion déléguée ou de secours, vues limitées au worker (ADR 0011, phase 3).

use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router as central_router},
    central_client::CentralClient,
    config::Config,
    worker_http::{WorkerState, WorkerWeb, router as worker_router},
};
use sqlx::sqlite::SqlitePoolOptions;
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

const HOST: &str = "localhost:3000";
const ORIGIN: &str = "http://localhost:3000";
const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;
const PASSWORD: &str = "un-bon-mot-de-passe";
const WORKER_NAME: &str = "nas-1";
const DEAD_CENTRAL: &str = "http://127.0.0.1:9";

fn config_in(dir: &Path) -> anyhow::Result<Config> {
    Ok(Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: dir.to_path_buf(),
        tools_cache_dir: dir.join("tools-cache"),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    })
}

async fn central_in(dir: &Path) -> anyhow::Result<axum::Router> {
    let config = config_in(dir)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    Ok(central_router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    }))
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: serde_json::Value,
}

async fn send(app: &axum::Router, request: Request<Body>) -> anyhow::Result<Reply> {
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
    Ok(Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    })
}

async fn post(
    app: &axum::Router,
    uri: &str,
    cookies: Option<&Cookies>,
    body: serde_json::Value,
) -> anyhow::Result<Reply> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "application/json");
    if let Some(cookies) = cookies {
        builder = builder
            .header("cookie", cookies.header())
            .header("x-csrf-token", cookies.csrf.clone());
    }
    send(app, builder.body(Body::from(serde_json::to_vec(&body)?))?).await
}

async fn get(
    app: &axum::Router,
    uri: &str,
    cookies: Option<&Cookies>,
    bearer: Option<&str>,
) -> anyhow::Result<Reply> {
    let mut builder = Request::builder().uri(uri).header("host", HOST);
    if let Some(cookies) = cookies {
        builder = builder.header("cookie", cookies.header());
    }
    if let Some(key) = bearer {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    send(app, builder.body(Body::empty())?).await
}

#[derive(Clone, Default)]
struct Cookies {
    session: String,
    csrf: String,
}

impl Cookies {
    fn from_headers(headers: &HeaderMap) -> Self {
        let mut cookies = Self::default();
        for value in headers.get_all("set-cookie") {
            let text = value.to_str().unwrap_or_default();
            let pair = text.split(';').next().unwrap_or_default();
            if let Some(token) = pair.strip_prefix("flux_session=") {
                cookies.session = token.to_owned();
            } else if let Some(token) = pair.strip_prefix("flux_csrf=") {
                cookies.csrf = token.to_owned();
            }
        }
        cookies
    }

    fn header(&self) -> String {
        format!("flux_session={}; flux_csrf={}", self.session, self.csrf)
    }
}

/// Un vrai central installé sur un port local, et la clé du worker `WORKER_NAME`.
struct Fixture {
    central_url: String,
    worker_key: String,
    central_app: axum::Router,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    _dir: tempfile::TempDir,
}

/// Chaque test a son administrateur : la limitation d'essais est partagée par tout le processus.
async fn fixture(admin: &str) -> anyhow::Result<Fixture> {
    let dir = tempfile::tempdir()?;
    let central_app = central_in(dir.path()).await?;
    let setup = post(
        &central_app,
        "/api/v1/setup/central",
        None,
        serde_json::json!({"name": "Central", "username": admin, "password": PASSWORD}),
    )
    .await?;
    assert_eq!(setup.status, StatusCode::OK);
    let enrolled = post(
        &central_app,
        "/api/v1/workers/enroll",
        None,
        serde_json::json!({"username": admin, "password": PASSWORD, "worker_name": WORKER_NAME}),
    )
    .await?;
    let worker_key = enrolled.body["key"].as_str().unwrap_or_default().to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let central_url = format!("http://{}", listener.local_addr()?);
    let served = central_app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, served).await });
    Ok(Fixture {
        central_url,
        worker_key,
        central_app,
        server,
        _dir: dir,
    })
}

fn worker_web(config_dir: &Path, central_url: &str, key: &str) -> anyhow::Result<Arc<WorkerWeb>> {
    let config = config_in(config_dir)?;
    let cache = config.tools_cache_dir.clone();
    Ok(WorkerWeb::new(
        WorkerState::new(WORKER_NAME, central_url),
        Arc::new(config),
        CentralClient::new(reqwest::Client::new(), central_url.parse()?, key.to_owned()),
        cache,
    ))
}

async fn login(app: &axum::Router, admin: &str, password: &str) -> anyhow::Result<Reply> {
    post(
        app,
        "/api/v1/auth/login",
        None,
        serde_json::json!({"username": admin, "password": password}),
    )
    .await
}

#[tokio::test]
async fn test_login_is_delegated_to_the_central_and_opens_a_memory_session() -> anyhow::Result<()> {
    let admin = "admin-delegue";
    let fixture = fixture(admin).await?;
    let dir = tempfile::tempdir()?;
    let worker = worker_router(worker_web(
        dir.path(),
        &fixture.central_url,
        &fixture.worker_key,
    )?);

    let wrong = login(&worker, admin, "mauvais-mot-de-passe").await?;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);

    let opened = login(&worker, admin, PASSWORD).await?;
    assert_eq!(opened.status, StatusCode::OK);
    let cookies = Cookies::from_headers(&opened.headers);
    assert!(!cookies.session.is_empty() && !cookies.csrf.is_empty());

    let me = get(&worker, "/api/v1/me", Some(&cookies), None).await?;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["instanceKind"], "worker");
    assert_eq!(me.body["role"], "admin");
    assert_eq!(me.body["name"], admin);
    assert_eq!(me.body["instanceName"], WORKER_NAME);

    let session = get(&worker, "/api/v1/auth/session", Some(&cookies), None).await?;
    assert_eq!(session.status, StatusCode::OK);
    assert_eq!(session.body["username"], admin);

    let no_csrf = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("cookie", cookies.header())
        .body(Body::empty())?;
    assert_eq!(send(&worker, no_csrf).await?.status, StatusCode::FORBIDDEN);
    let out = post(
        &worker,
        "/api/v1/auth/logout",
        Some(&cookies),
        serde_json::json!({}),
    )
    .await?;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = get(&worker, "/api/v1/me", Some(&cookies), None).await?;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    fixture.server.abort();
    Ok(())
}

#[tokio::test]
async fn test_views_need_a_session_and_come_from_the_worker_routes() -> anyhow::Result<()> {
    let admin = "admin-vues";
    let fixture = fixture(admin).await?;
    let dir = tempfile::tempdir()?;
    let worker = worker_router(worker_web(
        dir.path(),
        &fixture.central_url,
        &fixture.worker_key,
    )?);
    for path in [
        "/api/v1/worker/overview",
        "/api/v1/jobs",
        "/api/v1/plugins",
        "/api/v1/tools",
    ] {
        let reply = get(&worker, path, None, None).await?;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    let cookies = Cookies::from_headers(&login(&worker, admin, PASSWORD).await?.headers);

    let overview = get(&worker, "/api/v1/worker/overview", Some(&cookies), None).await?;
    assert_eq!(overview.status, StatusCode::OK);
    assert_eq!(overview.body["name"], WORKER_NAME);
    assert_eq!(overview.body["jobs"]["queued"], 0);

    let jobs = get(&worker, "/api/v1/jobs", Some(&cookies), None).await?;
    assert_eq!(jobs.status, StatusCode::OK);
    assert_eq!(jobs.body, serde_json::json!([]));

    for path in ["/api/v1/plugins", "/api/v1/tools"] {
        let reply = get(&worker, path, Some(&cookies), None).await?;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
        assert!(reply.body.is_array(), "{path}");
    }

    // Aucune route d'écriture ni d'administration n'existe sur le worker.
    let keys = get(&worker, "/api/v1/api-keys", Some(&cookies), None).await?;
    assert_eq!(keys.status, StatusCode::NOT_FOUND);
    fixture.server.abort();
    Ok(())
}

#[tokio::test]
async fn test_local_copy_logs_in_only_while_the_central_is_unreachable() -> anyhow::Result<()> {
    let admin = "admin-secours";
    let fixture = fixture(admin).await?;
    let dir = tempfile::tempdir()?;
    let live = worker_web(dir.path(), &fixture.central_url, &fixture.worker_key)?;
    live.sync_accounts()
        .await
        .map_err(|_| anyhow::anyhow!("synchronisation"))?;
    assert!(dir.path().join("accounts.json").is_file());
    let raw = std::fs::read_to_string(dir.path().join("accounts.json"))?;
    assert!(!raw.contains(PASSWORD));

    // Central coupé : la copie persistée (relue au démarrage) permet de se connecter.
    let isolated = worker_router(worker_web(dir.path(), DEAD_CENTRAL, &fixture.worker_key)?);
    let opened = login(&isolated, admin, PASSWORD).await?;
    assert_eq!(opened.status, StatusCode::OK);
    assert_eq!(
        login(&isolated, admin, "mauvais-mot-de-passe")
            .await?
            .status,
        StatusCode::UNAUTHORIZED
    );

    // Une copie vide ne donne accès à personne.
    let empty_dir = tempfile::tempdir()?;
    let blank = worker_router(worker_web(
        empty_dir.path(),
        DEAD_CENTRAL,
        &fixture.worker_key,
    )?);
    assert_eq!(
        login(&blank, admin, PASSWORD).await?.status,
        StatusCode::UNAUTHORIZED
    );

    // Les vues, elles, exigent le central.
    let cookies = Cookies::from_headers(&opened.headers);
    let jobs = get(&isolated, "/api/v1/jobs", Some(&cookies), None).await?;
    assert_eq!(jobs.status, StatusCode::BAD_GATEWAY);
    fixture.server.abort();
    Ok(())
}

#[tokio::test]
async fn test_central_worker_routes_serve_worker_keys_only() -> anyhow::Result<()> {
    let admin = "admin-routes";
    let fixture = fixture(admin).await?;
    let key = fixture.worker_key.as_str();
    for path in [
        "/api/v1/worker/overview",
        "/api/v1/worker/jobs",
        "/api/v1/worker/plugins",
        "/api/v1/worker/accounts",
    ] {
        let anonymous = get(&fixture.central_app, path, None, None).await?;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED, "{path}");
        let allowed = get(&fixture.central_app, path, None, Some(key)).await?;
        assert_eq!(allowed.status, StatusCode::OK, "{path}");
    }
    let accounts = get(
        &fixture.central_app,
        "/api/v1/worker/accounts",
        None,
        Some(key),
    )
    .await?;
    let list = accounts.body["accounts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["username"], admin);
    assert!(
        list[0]["password_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("$argon2"))
    );

    let login_url = "/api/v1/worker/login";
    let credentials = serde_json::json!({"username": admin, "password": PASSWORD});
    let anonymous = post(&fixture.central_app, login_url, None, credentials.clone()).await?;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let verified = send(
        &fixture.central_app,
        Request::builder()
            .method("POST")
            .uri(login_url)
            .header("host", HOST)
            .header("authorization", format!("Bearer {key}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&credentials)?))?,
    )
    .await?;
    assert_eq!(verified.status, StatusCode::OK);
    assert_eq!(verified.body["username"], admin);
    fixture.server.abort();
    Ok(())
}

#[tokio::test]
async fn test_health_and_status_stay_public_on_a_worker() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let worker = worker_router(worker_web(dir.path(), DEAD_CENTRAL, "fdf_cle")?);
    assert_eq!(
        get(&worker, "/health", None, None).await?.status,
        StatusCode::NO_CONTENT
    );
    let status = get(&worker, "/api/v1/instance/status", None, None).await?;
    assert_eq!(status.body["instanceKind"], "worker");
    assert_eq!(status.body["registered"], false);
    let setup = get(&worker, "/api/v1/setup/status", None, None).await?;
    assert_eq!(setup.body["needsSetup"], false);
    assert_eq!(setup.body["instanceKind"], "worker");
    Ok(())
}
