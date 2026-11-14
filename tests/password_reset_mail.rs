//! Mot de passe oublié par courriel (ADR 0015) : logique avec un expéditeur enregistré, puis routes.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    api_keys::hash_secret,
    config::{Config, SmtpConfig, SmtpTls},
    error::{AppError, AppResult},
    mail::{MailSender, OutgoingMail},
    password, password_reset, repository, web_session,
};
use flux_shared::Role;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const ORIGIN: &str = "http://flux.test";
const USERNAME: &str = "remi";
const EMAIL: &str = "remi@exemple.fr";
const OLD_PASSWORD: &str = "ancien-mot-de-passe";
const NEW_PASSWORD: &str = "nouveau-mot-de-passe";
const TOKEN_MARKER: &str = "#reset=";
const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;
const SESSION_TTL_SECONDS: i64 = 3_600;
const SENDS_PER_WINDOW: usize = 5;

/// Expéditeur qui garde les messages, ou échoue sur demande.
#[derive(Default)]
struct Outbox {
    sent: Mutex<Vec<OutgoingMail>>,
    fail: bool,
}

impl MailSender for Outbox {
    async fn send(&self, mail: OutgoingMail) -> AppResult<()> {
        if self.fail {
            return Err(AppError::Internal);
        }
        self.sent.lock().map_err(|_| AppError::Internal)?.push(mail);
        Ok(())
    }
}

impl Outbox {
    fn mails(&self) -> Vec<OutgoingMail> {
        self.sent
            .lock()
            .map(|sent| sent.clone())
            .unwrap_or_default()
    }

    /// Jeton du lien du premier message.
    fn token(&self) -> anyhow::Result<String> {
        let mails = self.mails();
        let body = &mails
            .first()
            .ok_or_else(|| anyhow::anyhow!("aucun message"))?
            .body;
        let start = body
            .find(TOKEN_MARKER)
            .ok_or_else(|| anyhow::anyhow!("sans lien"))?
            + TOKEN_MARKER.len();
        Ok(body[start..]
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect())
    }
}

async fn pool_with_account() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let hash = password::hash(OLD_PASSWORD.into()).await?;
    repository::create_first_account(&pool, USERNAME, &hash, Role::Admin, Some(EMAIL)).await?;
    Ok(pool)
}

async fn account_id(pool: &SqlitePool) -> anyhow::Result<String> {
    Ok(repository::find_account_by_email(pool, EMAIL)
        .await?
        .ok_or_else(|| anyhow::anyhow!("absent"))?
        .id)
}

async fn stored_hash(pool: &SqlitePool) -> anyhow::Result<String> {
    Ok(repository::get_account(pool, &account_id(pool).await?)
        .await?
        .ok_or_else(|| anyhow::anyhow!("absent"))?
        .1)
}

#[tokio::test]
async fn test_a_known_address_gets_one_mail_with_a_working_single_use_link() -> anyhow::Result<()> {
    let pool = pool_with_account().await?;
    let outbox = Outbox::default();
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, "REMI@exemple.fr").await?;
    let mails = outbox.mails();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, "REMI@exemple.fr");
    assert!(mails[0].body.contains(&format!("{ORIGIN}/{TOKEN_MARKER}")));
    let token = outbox.token()?;
    // Seul le haché est en base.
    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM password_reset_tokens")
        .fetch_all(&pool)
        .await?;
    assert_eq!(stored, vec![hash_secret(&token)]);
    password_reset::confirm_reset(&pool, &token, NEW_PASSWORD).await?;
    assert!(password::verify(NEW_PASSWORD.into(), stored_hash(&pool).await?).await);
    assert!(matches!(
        password_reset::confirm_reset(&pool, &token, "autre-mot-de-passe").await,
        Err(AppError::Invalid)
    ));
    Ok(())
}

#[tokio::test]
async fn test_unknown_and_disabled_addresses_send_nothing_and_succeed() -> anyhow::Result<()> {
    let pool = pool_with_account().await?;
    let outbox = Outbox::default();
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, "inconnu@exemple.fr").await?;
    let id = account_id(&pool).await?;
    sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = ?1")
        .bind(&id)
        .execute(&pool)
        .await?;
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, EMAIL).await?;
    assert!(outbox.mails().is_empty());
    let tokens: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens")
        .fetch_one(&pool)
        .await?;
    assert_eq!(tokens, 0);
    Ok(())
}

#[tokio::test]
async fn test_a_second_request_within_the_cooldown_sends_nothing() -> anyhow::Result<()> {
    let pool = pool_with_account().await?;
    let outbox = Outbox::default();
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, EMAIL).await?;
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, EMAIL).await?;
    assert_eq!(outbox.mails().len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_a_new_request_replaces_older_tokens_once_the_cooldown_is_over() -> anyhow::Result<()>
{
    let pool = pool_with_account().await?;
    let outbox = Outbox::default();
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, EMAIL).await?;
    let first = outbox.token()?;
    sqlx::query("UPDATE password_reset_tokens SET created_at = datetime('now', '-1 hour')")
        .execute(&pool)
        .await?;
    password_reset::issue_and_send(&pool, &outbox, ORIGIN, EMAIL).await?;
    assert_eq!(outbox.mails().len(), 2);
    assert!(matches!(
        password_reset::confirm_reset(&pool, &first, NEW_PASSWORD).await,
        Err(AppError::Invalid)
    ));
    Ok(())
}

#[tokio::test]
async fn test_a_mail_failure_is_swallowed() -> anyhow::Result<()> {
    let pool = pool_with_account().await?;
    let outbox = Outbox {
        fail: true,
        ..Outbox::default()
    };
    assert!(
        password_reset::issue_and_send(&pool, &outbox, ORIGIN, EMAIL)
            .await
            .is_ok()
    );
    Ok(())
}

#[tokio::test]
async fn test_a_weak_password_keeps_the_token_and_an_expired_token_is_refused() -> anyhow::Result<()>
{
    let pool = pool_with_account().await?;
    let id = account_id(&pool).await?;
    repository::insert_reset_token(&pool, &hash_secret("valide"), &id, 600).await?;
    assert!(matches!(
        password_reset::confirm_reset(&pool, "valide", "court").await,
        Err(AppError::Invalid)
    ));
    assert!(
        password_reset::confirm_reset(&pool, " valide ", NEW_PASSWORD)
            .await
            .is_ok()
    );
    repository::insert_reset_token(&pool, &hash_secret("expire"), &id, 0).await?;
    assert!(matches!(
        password_reset::confirm_reset(&pool, "expire", NEW_PASSWORD).await,
        Err(AppError::Invalid)
    ));
    assert!(matches!(
        password_reset::confirm_reset(&pool, "inconnu", NEW_PASSWORD).await,
        Err(AppError::Invalid)
    ));
    Ok(())
}

#[tokio::test]
async fn test_confirming_closes_every_session_and_a_disabled_account_is_refused()
-> anyhow::Result<()> {
    let pool = pool_with_account().await?;
    let id = account_id(&pool).await?;
    web_session::create(&pool, SESSION_TTL_SECONDS, &id).await?;
    repository::insert_reset_token(&pool, &hash_secret("t1"), &id, 600).await?;
    password_reset::confirm_reset(&pool, "t1", NEW_PASSWORD).await?;
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE account_id = ?1")
        .bind(&id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(sessions, 0);
    sqlx::query("UPDATE accounts SET disabled = 1 WHERE id = ?1")
        .bind(&id)
        .execute(&pool)
        .await?;
    repository::insert_reset_token(&pool, &hash_secret("t2"), &id, 600).await?;
    assert!(matches!(
        password_reset::confirm_reset(&pool, "t2", "encore-un-mot-de-passe").await,
        Err(AppError::Invalid)
    ));
    Ok(())
}

#[tokio::test]
async fn test_email_addresses_are_unique_ignoring_case_and_clearable() -> anyhow::Result<()> {
    let pool = pool_with_account().await?;
    let hash = password::hash(OLD_PASSWORD.into()).await?;
    let other = repository::create_account(&pool, "autre", &hash, Role::Viewer, None).await?;
    assert!(matches!(
        repository::set_account_email(&pool, &other.id, Some("REMI@exemple.fr")).await,
        Err(AppError::Conflict)
    ));
    repository::set_account_email(&pool, &other.id, Some("autre@exemple.fr")).await?;
    assert_eq!(
        repository::find_account_by_email(&pool, "AUTRE@exemple.fr")
            .await?
            .map(|a| a.id),
        Some(other.id.clone())
    );
    repository::set_account_email(&pool, &other.id, None).await?;
    assert!(
        repository::find_account_by_email(&pool, "autre@exemple.fr")
            .await?
            .is_none()
    );
    assert!(matches!(
        repository::set_account_email(&pool, "absent", None).await,
        Err(AppError::NotFound)
    ));
    Ok(())
}

fn config(smtp: bool) -> anyhow::Result<Config> {
    Ok(Config {
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
        web_origin: Some(ORIGIN.into()),
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: smtp.then(|| SmtpConfig {
            host: "127.0.0.1".into(),
            port: 1,
            tls: SmtpTls::None,
            username: None,
            password: None,
            from: "flux@exemple.fr".into(),
        }),
    })
}

async fn app(smtp: bool) -> anyhow::Result<(axum::Router, SqlitePool)> {
    let pool = pool_with_account().await?;
    common::install_keys(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(8);
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config(smtp)?),
        events_tx,
    };
    Ok((router(state), pool))
}

async fn post(
    app: &axum::Router,
    uri: &str,
    body: serde_json::Value,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    send(app, "POST", uri, Some(body), None).await
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    bearer: Option<&str>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "flux.test")
        .header("origin", ORIGIN);
    if let Some(key) = bearer {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    let request = match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&json)?))?,
        None => builder.body(Body::empty())?,
    };
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    ))
}

#[tokio::test]
async fn test_forgot_answers_the_same_for_unknown_addresses_and_validates_the_format()
-> anyhow::Result<()> {
    let (app, _) = app(true).await?;
    let (status, _) = post(
        &app,
        "/api/v1/auth/forgot-password",
        serde_json::json!({"email": "personne@exemple.fr"}),
    )
    .await?;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (status, _) = post(
        &app,
        "/api/v1/auth/forgot-password",
        serde_json::json!({"email": "pas-une-adresse"}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn test_forgot_is_unavailable_without_smtp_and_rejects_foreign_origins() -> anyhow::Result<()>
{
    let (disabled, _) = app(false).await?;
    let (status, _) = post(
        &disabled,
        "/api/v1/auth/forgot-password",
        serde_json::json!({"email": EMAIL}),
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (enabled, _) = app(true).await?;
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/forgot-password")
        .header("host", "flux.test")
        .header("origin", "http://evil.test")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"a@b.fr"}"#))?;
    assert_eq!(
        enabled.clone().oneshot(request).await?.status(),
        StatusCode::FORBIDDEN
    );
    Ok(())
}

#[tokio::test]
async fn test_forgot_is_rate_limited_per_address() -> anyhow::Result<()> {
    let (app, _) = app(true).await?;
    let body = serde_json::json!({"email": "limite@exemple.fr"});
    for _ in 0..SENDS_PER_WINDOW {
        assert_eq!(
            post(&app, "/api/v1/auth/forgot-password", body.clone())
                .await?
                .0,
            StatusCode::ACCEPTED
        );
    }
    assert_eq!(
        post(&app, "/api/v1/auth/forgot-password", body).await?.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    Ok(())
}

#[tokio::test]
async fn test_reset_route_changes_the_password_once() -> anyhow::Result<()> {
    let (app, pool) = app(true).await?;
    repository::insert_reset_token(
        &pool,
        &hash_secret("jeton-route"),
        &account_id(&pool).await?,
        600,
    )
    .await?;
    let body = serde_json::json!({"token": "jeton-route", "new_password": NEW_PASSWORD});
    assert_eq!(
        post(&app, "/api/v1/auth/reset-password", body.clone())
            .await?
            .0,
        StatusCode::NO_CONTENT
    );
    let (status, _) = post(
        &app,
        "/api/v1/auth/login",
        serde_json::json!({"username": USERNAME, "password": NEW_PASSWORD}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        post(&app, "/api/v1/auth/reset-password", body).await?.0,
        StatusCode::BAD_REQUEST
    );
    Ok(())
}

#[tokio::test]
async fn test_setup_status_tells_whether_the_mail_reset_is_available() -> anyhow::Result<()> {
    for smtp in [true, false] {
        let (app, _) = app(smtp).await?;
        let (_, body) = send(&app, "GET", "/api/v1/setup/status", None, None).await?;
        assert_eq!(body["passwordResetEnabled"], smtp);
    }
    Ok(())
}

#[tokio::test]
async fn test_admins_set_clear_and_list_account_emails() -> anyhow::Result<()> {
    let (app, _) = app(false).await?;
    let key = Some(common::ADMIN_KEY);
    let (status, created) = send(&app, "POST", "/api/v1/accounts", Some(serde_json::json!({
        "username": "lecteur", "password": NEW_PASSWORD, "role": "viewer", "email": "lecteur@exemple.fr"
    })), key).await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["email"], "lecteur@exemple.fr");
    let id = created["id"].as_str().unwrap_or_default().to_owned();
    let uri = format!("/api/v1/accounts/{id}");
    let (status, _) = send(
        &app,
        "PUT",
        &uri,
        Some(serde_json::json!({"email": EMAIL})),
        key,
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = send(
        &app,
        "PUT",
        &uri,
        Some(serde_json::json!({"email": "pas-une-adresse"})),
        key,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, updated) = send(
        &app,
        "PUT",
        &uri,
        Some(serde_json::json!({"email": ""})),
        key,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(updated.get("email").is_none());
    let (status, _) = send(&app, "PUT", &uri, Some(serde_json::json!({})), key).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}
