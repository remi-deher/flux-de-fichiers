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

fn app() -> anyhow::Result<axum::Router> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key-not-a-real-credential".into(),
        worker_key: "test-worker-key-not-a-real-credential".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    let pool = SqlitePoolOptions::new().connect_lazy(&config.database_url)?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    Ok(router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    }))
}

#[tokio::test]
async fn protected_routes_reject_missing_keys_before_accessing_resources() -> anyhow::Result<()> {
    for path in [
        "/api/v1/jobs",
        "/api/v1/jobs/events",
        "/api/v1/files",
        "/api/v1/plugins",
        "/api/v1/tools",
        "/api/v1/config",
        "/api/v1/auth/session",
        "/api/v1/workers",
        "/api/v1/jobs/00000000-0000-0000-0000-000000000001/output",
        "/api/v1/jobs/00000000-0000-0000-0000-000000000001/source",
    ] {
        let response = app()?
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        assert!(response.headers().contains_key("x-request-id"));
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await?)?;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }
    Ok(())
}

#[tokio::test]
async fn session_login_rejects_untrusted_origin_and_invalid_key() -> anyhow::Result<()> {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/session")
        .header("host", "localhost:3000")
        .header("content-type", "application/json")
        .header("origin", "https://attacker.example")
        .body(Body::from(r#"{"admin_key":"not-the-admin-key"}"#))?;
    let response = app()?.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/session")
        .header("host", "localhost:3000")
        .header("content-type", "application/json")
        .header("origin", "http://localhost:3000")
        .body(Body::from(r#"{"admin_key":"not-the-admin-key"}"#))?;
    let response = app()?.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn session_login_accepts_nas_ip_when_web_origin_not_set() -> anyhow::Result<()> {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/session")
        .header("host", "192.168.1.50:3000")
        .header("content-type", "application/json")
        .header("origin", "http://192.168.1.50:3000")
        .body(Body::from(r#"{"admin_key":"not-the-admin-key"}"#))?;
    let response = app()?.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/session")
        .header("host", "192.168.1.50:3000")
        .header("content-type", "application/json")
        .header("origin", "http://attacker.example")
        .body(Body::from(r#"{"admin_key":"not-the-admin-key"}"#))?;
    let response = app()?.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    Ok(())
}

#[tokio::test]
async fn explicit_web_origin_strictly_enforces_configured_origin() -> anyhow::Result<()> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key-not-a-real-credential".into(),
        worker_key: "test-worker-key-not-a-real-credential".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: Some("http://custom.domain.local:3000".into()),
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    let pool = SqlitePoolOptions::new().connect_lazy(&config.database_url)?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    let app = router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    });

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/session")
        .header("host", "custom.domain.local:3000")
        .header("content-type", "application/json")
        .header("origin", "http://custom.domain.local:3000")
        .body(Body::from(r#"{"admin_key":"not-the-admin-key"}"#))?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/session")
        .header("host", "other.domain.local:3000")
        .header("content-type", "application/json")
        .header("origin", "http://other.domain.local:3000")
        .body(Body::from(r#"{"admin_key":"not-the-admin-key"}"#))?;
    let response = app.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    Ok(())
}

#[tokio::test]
async fn authorization_header_takes_priority_over_web_session_cookie() -> anyhow::Result<()> {
    use axum::http::{HeaderMap, Method};
    use flux_de_fichiers::web_session::authorize_admin;
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key-not-a-real-credential".into(),
        worker_key: "test-worker-key-not-a-real-credential".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    let pool = SqlitePoolOptions::new().connect_lazy(&config.database_url)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        "Bearer test-admin-key-not-a-real-credential".parse()?,
    );
    assert!(
        authorize_admin(&pool, &config, &headers, &Method::GET)
            .await
            .is_ok()
    );

    headers.insert("authorization", "Bearer invalid".parse()?);
    headers.insert("cookie", "flux_session=valid-session-cookie".parse()?);
    assert!(matches!(
        authorize_admin(&pool, &config, &headers, &Method::GET).await,
        Err(flux_de_fichiers::error::AppError::Unauthorized)
    ));
    Ok(())
}

#[test]
fn csrf_requires_matching_cookie_header_and_server_verifier() -> anyhow::Result<()> {
    use chrono::Utc;
    use flux_de_fichiers::web_session::{self, ValidatedSession};
    use sha2::{Digest, Sha256};
    let token = "csrf-token-for-test";
    let csrf_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let session = ValidatedSession {
        token_hash: "opaque-session-hash".into(),
        csrf_hash,
        expires_at: Utc::now(),
    };
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("cookie", format!("flux_csrf={token}").parse()?);
    headers.insert("x-csrf-token", token.parse()?);
    assert!(web_session::validate_csrf(&headers, &session).is_ok());

    headers.insert("x-csrf-token", "wrong-token".parse()?);
    assert!(web_session::validate_csrf(&headers, &session).is_err());
    Ok(())
}

#[test]
fn web_cookies_have_scoped_flags_and_secure_on_https() -> anyhow::Result<()> {
    use chrono::Utc;
    use flux_de_fichiers::{
        config::Config,
        web_session::{self, WebSession},
    };
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key-not-a-real-credential".into(),
        worker_key: "test-worker-key-not-a-real-credential".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: Some("https://localhost:3000".into()),
        web_session_ttl_seconds: 600,
        tls: None,
    };
    let session = WebSession {
        token: "opaque-session-token".into(),
        csrf_token: "csrf-token".into(),
        expires_at: Utc::now(),
    };
    let session_cookie = web_session::session_cookie(&session, &config);
    assert!(session_cookie.contains("HttpOnly"));
    assert!(session_cookie.contains("Secure"));
    assert!(session_cookie.contains("SameSite=Strict"));
    assert!(session_cookie.contains("Max-Age=600"));
    let csrf_cookie = web_session::csrf_cookie(&session, &config);
    assert!(!csrf_cookie.contains("HttpOnly"));
    assert!(csrf_cookie.contains("Secure"));
    assert!(web_session::clear_cookies(&config)[0].contains("Secure"));
    Ok(())
}

#[tokio::test]
async fn malformed_identifiers_and_unknown_routes_have_safe_json_errors() -> anyhow::Result<()> {
    for (path, status) in [
        ("/api/v1/jobs/invalid", StatusCode::BAD_REQUEST),
        ("/api/v1/unknown", StatusCode::NOT_FOUND),
    ] {
        let response = app()?
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["content-type"], "application/json");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await?)?;
        assert_eq!(body["error"]["code"], "invalid_request");
    }

    // SPA fallback: non-API route returns 200 OK when frontend dist/index.html is present
    let response = app()?
        .oneshot(Request::builder().uri("/unknown").body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}
