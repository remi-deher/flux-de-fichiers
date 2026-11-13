//! Installation, connexion, sessions, clés d'API et droits (ADR 0011).

mod common;

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
const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;
const GOOD_PASSWORD: &str = "un-bon-mot-de-passe";

async fn fresh_app(with_known_keys: bool) -> anyhow::Result<axum::Router> {
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
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    if with_known_keys {
        common::install_keys(&pool).await?;
    }
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    Ok(router(AppState {
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

/// Cookies d'une session web ouverte : de quoi rejouer des requêtes de navigateur.
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

enum Auth<'a> {
    None,
    Bearer(&'a str),
    /// Session web, avec ou sans l'en-tête CSRF.
    Session(&'a Cookies, bool),
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    auth: Auth<'_>,
    body: Option<serde_json::Value>,
) -> anyhow::Result<Reply> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", HOST)
        .header("origin", ORIGIN);
    match auth {
        Auth::None => {}
        Auth::Bearer(key) => builder = builder.header("authorization", format!("Bearer {key}")),
        Auth::Session(cookies, with_csrf) => {
            builder = builder.header("cookie", cookies.header());
            if with_csrf {
                builder = builder.header("x-csrf-token", cookies.csrf.clone());
            }
        }
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
    Ok(Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    })
}

async fn setup(app: &axum::Router, username: &str) -> anyhow::Result<Cookies> {
    let reply = call(
        app,
        "POST",
        "/api/v1/setup/central",
        Auth::None,
        Some(serde_json::json!({"name": "Mon central", "username": username, "password": GOOD_PASSWORD})),
    )
    .await?;
    assert_eq!(reply.status, StatusCode::OK);
    Ok(Cookies::from_headers(&reply.headers))
}

async fn login(app: &axum::Router, username: &str, password: &str) -> anyhow::Result<Reply> {
    call(
        app,
        "POST",
        "/api/v1/auth/login",
        Auth::None,
        Some(serde_json::json!({"username": username, "password": password})),
    )
    .await
}

#[tokio::test]
async fn test_setup_creates_the_admin_once_and_opens_a_session() -> anyhow::Result<()> {
    let app = fresh_app(false).await?;
    let status = call(&app, "GET", "/api/v1/setup/status", Auth::None, None).await?;
    assert_eq!(status.body["needsSetup"], true);
    assert_eq!(status.body["instanceKind"], "central");

    for bad in [
        serde_json::json!({"name": "", "username": "admin1", "password": GOOD_PASSWORD}),
        serde_json::json!({"name": "Central", "username": "a", "password": GOOD_PASSWORD}),
        serde_json::json!({"name": "Central", "username": "admin1", "password": "court"}),
    ] {
        let reply = call(&app, "POST", "/api/v1/setup/central", Auth::None, Some(bad)).await?;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    }

    let cookies = setup(&app, "setup-admin").await?;
    assert!(!cookies.session.is_empty() && !cookies.csrf.is_empty());

    let status = call(&app, "GET", "/api/v1/setup/status", Auth::None, None).await?;
    assert_eq!(status.body["needsSetup"], false);
    assert_eq!(status.body["instanceName"], "Mon central");

    let again = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        Auth::None,
        Some(serde_json::json!({"name": "Autre", "username": "intrus", "password": GOOD_PASSWORD})),
    )
    .await?;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.body["error"]["code"], "already_configured");

    let session = call(
        &app,
        "GET",
        "/api/v1/auth/session",
        Auth::Session(&cookies, false),
        None,
    )
    .await?;
    assert_eq!(session.status, StatusCode::OK);
    assert_eq!(session.body["username"], "setup-admin");

    let me = call(
        &app,
        "GET",
        "/api/v1/me",
        Auth::Session(&cookies, false),
        None,
    )
    .await?;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["role"], "admin");
    assert_eq!(me.body["instanceKind"], "central");
    assert_eq!(me.body["instanceName"], "Mon central");
    let permissions = me.body["permissions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(permissions.iter().any(|p| p == "manage_api_keys"));
    assert!(!permissions.iter().any(|p| p == "worker_protocol"));
    Ok(())
}

#[tokio::test]
async fn test_login_accepts_the_right_password_only_and_never_reveals_which_part_failed()
-> anyhow::Result<()> {
    let app = fresh_app(false).await?;
    setup(&app, "login-admin").await?;

    let ok = login(&app, "LOGIN-ADMIN", GOOD_PASSWORD).await?;
    assert_eq!(ok.status, StatusCode::OK);
    assert!(!Cookies::from_headers(&ok.headers).session.is_empty());

    let wrong_password = login(&app, "login-admin", "pas-le-bon-mot-de-passe").await?;
    let unknown_user = login(&app, "personne", GOOD_PASSWORD).await?;
    assert_eq!(wrong_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_user.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong_password.body, unknown_user.body);
    Ok(())
}

#[tokio::test]
async fn test_repeated_failures_lock_the_account_temporarily() -> anyhow::Result<()> {
    let app = fresh_app(false).await?;
    setup(&app, "locked-admin").await?;
    for _ in 0..5 {
        let reply = login(&app, "locked-admin", "pas-le-bon-mot-de-passe").await?;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }
    let blocked = login(&app, "locked-admin", GOOD_PASSWORD).await?;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.body["error"]["code"], "too_many_attempts");
    Ok(())
}

#[tokio::test]
async fn test_a_session_needs_csrf_to_change_anything_but_not_to_read() -> anyhow::Result<()> {
    let app = fresh_app(false).await?;
    let cookies = setup(&app, "csrf-admin").await?;
    let body = Some(serde_json::json!({"name": "Mon worker", "role": "worker"}));

    let without = call(
        &app,
        "POST",
        "/api/v1/api-keys",
        Auth::Session(&cookies, false),
        body.clone(),
    )
    .await?;
    assert_eq!(without.status, StatusCode::FORBIDDEN);
    let with = call(
        &app,
        "POST",
        "/api/v1/api-keys",
        Auth::Session(&cookies, true),
        body,
    )
    .await?;
    assert_eq!(with.status, StatusCode::OK);
    let read = call(
        &app,
        "GET",
        "/api/v1/api-keys",
        Auth::Session(&cookies, false),
        None,
    )
    .await?;
    assert_eq!(read.status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn test_api_keys_are_shown_once_scoped_by_role_and_revocable() -> anyhow::Result<()> {
    let app = fresh_app(true).await?;
    let admin = common::ADMIN_KEY;

    let created = call(
        &app,
        "POST",
        "/api/v1/api-keys",
        Auth::Bearer(admin),
        Some(serde_json::json!({"name": "Worker NAS", "role": "worker"})),
    )
    .await?;
    assert_eq!(created.status, StatusCode::OK);
    let worker_key = created.body["key"].as_str().unwrap_or_default().to_owned();
    let key_id = created.body["id"].as_str().unwrap_or_default().to_owned();
    assert!(worker_key.starts_with("fdf_"));
    assert_eq!(created.body["role"], "worker");

    let listed = call(&app, "GET", "/api/v1/api-keys", Auth::Bearer(admin), None).await?;
    let text = listed.body.to_string();
    assert!(!text.contains(&worker_key));
    assert!(!text.contains("keyHash") && !text.contains("key_hash"));
    assert!(text.contains("Worker NAS"));

    // Un worker n'administre rien ; un administrateur n'est pas un worker.
    let as_worker = call(
        &app,
        "GET",
        "/api/v1/api-keys",
        Auth::Bearer(&worker_key),
        None,
    )
    .await?;
    assert_eq!(as_worker.status, StatusCode::FORBIDDEN);
    let agent_route = "/api/v1/worker/tools/app.absent/distribution";
    let agent_ok = call(&app, "GET", agent_route, Auth::Bearer(&worker_key), None).await?;
    assert_eq!(agent_ok.status, StatusCode::NOT_FOUND);
    let agent_by_admin = call(&app, "GET", agent_route, Auth::Bearer(admin), None).await?;
    assert_eq!(agent_by_admin.status, StatusCode::FORBIDDEN);
    let anonymous = call(&app, "GET", agent_route, Auth::None, None).await?;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    let revoked = call(
        &app,
        "DELETE",
        &format!("/api/v1/api-keys/{key_id}"),
        Auth::Bearer(admin),
        None,
    )
    .await?;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let after = call(&app, "GET", agent_route, Auth::Bearer(&worker_key), None).await?;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    let again = call(
        &app,
        "DELETE",
        &format!("/api/v1/api-keys/{key_id}"),
        Auth::Bearer(admin),
        None,
    )
    .await?;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn test_api_key_creation_validates_role_and_name() -> anyhow::Result<()> {
    let app = fresh_app(true).await?;
    for body in [
        serde_json::json!({"name": "Clé", "role": "root"}),
        serde_json::json!({"name": "", "role": "worker"}),
        serde_json::json!({"name": "x".repeat(81), "role": "worker"}),
    ] {
        let reply = call(
            &app,
            "POST",
            "/api/v1/api-keys",
            Auth::Bearer(common::ADMIN_KEY),
            Some(body),
        )
        .await?;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    }
    Ok(())
}

#[tokio::test]
async fn test_changing_the_password_closes_other_sessions_and_keeps_the_current_one()
-> anyhow::Result<()> {
    let app = fresh_app(false).await?;
    let current = setup(&app, "pwd-admin").await?;
    let other = Cookies::from_headers(&login(&app, "pwd-admin", GOOD_PASSWORD).await?.headers);

    let change = |current_password: &str| {
        Some(
            serde_json::json!({"current_password": current_password, "new_password": "nouveau-mot-de-passe"}),
        )
    };
    let wrong = call(
        &app,
        "PUT",
        "/api/v1/account/password",
        Auth::Session(&current, true),
        change("faux-mot-de-passe"),
    )
    .await?;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    let weak = call(
        &app,
        "PUT",
        "/api/v1/account/password",
        Auth::Session(&current, true),
        Some(serde_json::json!({"current_password": GOOD_PASSWORD, "new_password": "court"})),
    )
    .await?;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);

    let done = call(
        &app,
        "PUT",
        "/api/v1/account/password",
        Auth::Session(&current, true),
        change(GOOD_PASSWORD),
    )
    .await?;
    assert_eq!(done.status, StatusCode::NO_CONTENT);

    assert_eq!(
        call(
            &app,
            "GET",
            "/api/v1/me",
            Auth::Session(&current, false),
            None
        )
        .await?
        .status,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/v1/me",
            Auth::Session(&other, false),
            None
        )
        .await?
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "pwd-admin", GOOD_PASSWORD).await?.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "pwd-admin", "nouveau-mot-de-passe")
            .await?
            .status,
        StatusCode::OK
    );
    Ok(())
}

#[tokio::test]
async fn test_logout_revokes_the_session() -> anyhow::Result<()> {
    let app = fresh_app(false).await?;
    let cookies = setup(&app, "logout-admin").await?;
    let out = call(
        &app,
        "POST",
        "/api/v1/auth/logout",
        Auth::Session(&cookies, true),
        None,
    )
    .await?;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let me = call(
        &app,
        "GET",
        "/api/v1/me",
        Auth::Session(&cookies, false),
        None,
    )
    .await?;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    Ok(())
}
