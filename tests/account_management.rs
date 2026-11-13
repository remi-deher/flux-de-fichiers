//! Gestion des comptes et rôle lecteur (ADR 0011).

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
const PASSWORD: &str = "un-bon-mot-de-passe";
const ADMIN: &str = "admin-comptes";

async fn fresh_app() -> anyhow::Result<(axum::Router, tempfile::TempDir)> {
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
    let app = router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    });
    Ok((app, dir))
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: serde_json::Value,
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
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookies: Option<&Cookies>,
    body: Option<serde_json::Value>,
) -> anyhow::Result<Reply> {
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
    Ok(Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    })
}

async fn login(app: &axum::Router, username: &str) -> anyhow::Result<Reply> {
    call(
        app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({"username": username, "password": PASSWORD})),
    )
    .await
}

/// Centrale installée : l'administrateur est connecté.
async fn installed() -> anyhow::Result<(axum::Router, Cookies, tempfile::TempDir)> {
    let (app, dir) = fresh_app().await?;
    let reply = call(
        &app,
        "POST",
        "/api/v1/setup/central",
        None,
        Some(serde_json::json!({"name": "Centrale", "username": ADMIN, "password": PASSWORD})),
    )
    .await?;
    assert_eq!(reply.status, StatusCode::OK);
    Ok((app, Cookies::from_headers(&reply.headers), dir))
}

async fn create(
    app: &axum::Router,
    admin: &Cookies,
    username: &str,
    role: &str,
) -> anyhow::Result<Reply> {
    call(
        app,
        "POST",
        "/api/v1/accounts",
        Some(admin),
        Some(serde_json::json!({"username": username, "password": PASSWORD, "role": role})),
    )
    .await
}

async fn account_id(app: &axum::Router, admin: &Cookies, username: &str) -> anyhow::Result<String> {
    let list = call(app, "GET", "/api/v1/accounts", Some(admin), None).await?;
    let found = list.body.as_array().and_then(|accounts| {
        accounts
            .iter()
            .find(|account| account["username"] == username)
    });
    Ok(found
        .and_then(|account| account["id"].as_str())
        .unwrap_or_default()
        .to_owned())
}

#[tokio::test]
async fn test_admin_creates_lists_and_validates_accounts() -> anyhow::Result<()> {
    let (app, admin, _dir) = installed().await?;
    let created = create(&app, &admin, "lecteur-1", "viewer").await?;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["role"], "viewer");
    assert_eq!(created.body["disabled"], false);

    let list = call(&app, "GET", "/api/v1/accounts", Some(&admin), None).await?;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body.as_array().map(Vec::len), Some(2));
    assert!(!list.body.to_string().contains("argon2"));
    assert!(!list.body.to_string().contains("password"));

    let duplicate = create(&app, &admin, "LECTEUR-1", "viewer").await?;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    for (name, role) in [("machine-1", "worker"), ("autre-1", "root")] {
        let refused = create(&app, &admin, name, role).await?;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{role}");
    }
    let weak = call(
        &app,
        "POST",
        "/api/v1/accounts",
        Some(&admin),
        Some(serde_json::json!({"username": "faible-1", "password": "court", "role": "viewer"})),
    )
    .await?;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn test_viewer_can_look_but_not_change_anything() -> anyhow::Result<()> {
    let (app, admin, _dir) = installed().await?;
    create(&app, &admin, "lecteur-2", "viewer").await?;
    let opened = login(&app, "lecteur-2").await?;
    assert_eq!(opened.status, StatusCode::OK);
    let viewer = Cookies::from_headers(&opened.headers);

    let me = call(&app, "GET", "/api/v1/me", Some(&viewer), None).await?;
    assert_eq!(me.body["role"], "viewer");
    let permissions: Vec<String> = me.body["permissions"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(permissions.contains(&"view_jobs".to_owned()));
    assert!(!permissions.contains(&"manage_settings".to_owned()));

    for path in [
        "/api/v1/jobs",
        "/api/v1/flows",
        "/api/v1/libraries",
        "/api/v1/workers",
        "/api/v1/config",
        "/api/v1/plugins",
        "/api/v1/tools",
        "/api/v1/plugins/repositories",
    ] {
        let reply = call(&app, "GET", path, Some(&viewer), None).await?;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
    }
    for path in ["/api/v1/api-keys", "/api/v1/accounts"] {
        let reply = call(&app, "GET", path, Some(&viewer), None).await?;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{path}");
    }
    let forbidden = create(&app, &viewer, "intrus-1", "admin").await?;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    let write = call(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(&viewer),
        Some(serde_json::json!({"name": "x", "path": "/x"})),
    )
    .await?;
    assert_eq!(write.status, StatusCode::FORBIDDEN);
    Ok(())
}

#[tokio::test]
async fn test_the_last_active_administrator_cannot_be_removed() -> anyhow::Result<()> {
    let (app, admin, _dir) = installed().await?;
    let own_id = account_id(&app, &admin, ADMIN).await?;
    let uri = format!("/api/v1/accounts/{own_id}");
    for patch in [
        serde_json::json!({"disabled": true}),
        serde_json::json!({"role": "viewer"}),
    ] {
        let refused = call(&app, "PUT", &uri, Some(&admin), Some(patch)).await?;
        assert_eq!(refused.status, StatusCode::CONFLICT);
    }
    // Avec un second administrateur, le premier peut être rétrogradé.
    create(&app, &admin, "admin-2", "admin").await?;
    let demoted = call(
        &app,
        "PUT",
        &uri,
        Some(&admin),
        Some(serde_json::json!({"role": "viewer"})),
    )
    .await?;
    assert_eq!(demoted.status, StatusCode::OK);
    assert_eq!(demoted.body["role"], "viewer");
    Ok(())
}

#[tokio::test]
async fn test_disabling_closes_sessions_and_blocks_login_until_reactivated() -> anyhow::Result<()> {
    let (app, admin, _dir) = installed().await?;
    create(&app, &admin, "lecteur-3", "viewer").await?;
    let viewer = Cookies::from_headers(&login(&app, "lecteur-3").await?.headers);
    assert_eq!(
        call(&app, "GET", "/api/v1/jobs", Some(&viewer), None)
            .await?
            .status,
        StatusCode::OK
    );

    let id = account_id(&app, &admin, "lecteur-3").await?;
    let uri = format!("/api/v1/accounts/{id}");
    let disabled = call(
        &app,
        "PUT",
        &uri,
        Some(&admin),
        Some(serde_json::json!({"disabled": true})),
    )
    .await?;
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(disabled.body["disabled"], true);
    assert_eq!(
        call(&app, "GET", "/api/v1/jobs", Some(&viewer), None)
            .await?
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "lecteur-3").await?.status,
        StatusCode::UNAUTHORIZED
    );

    call(
        &app,
        "PUT",
        &uri,
        Some(&admin),
        Some(serde_json::json!({"disabled": false})),
    )
    .await?;
    assert_eq!(login(&app, "lecteur-3").await?.status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn test_invalid_updates_are_refused() -> anyhow::Result<()> {
    let (app, admin, _dir) = installed().await?;
    let id = account_id(&app, &admin, ADMIN).await?;
    let empty = call(
        &app,
        "PUT",
        &format!("/api/v1/accounts/{id}"),
        Some(&admin),
        Some(serde_json::json!({})),
    )
    .await?;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    let worker_role = call(
        &app,
        "PUT",
        &format!("/api/v1/accounts/{id}"),
        Some(&admin),
        Some(serde_json::json!({"role": "worker"})),
    )
    .await?;
    assert_eq!(worker_role.status, StatusCode::BAD_REQUEST);
    let unknown = call(
        &app,
        "PUT",
        "/api/v1/accounts/inconnu",
        Some(&admin),
        Some(serde_json::json!({"disabled": true})),
    )
    .await?;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    Ok(())
}
