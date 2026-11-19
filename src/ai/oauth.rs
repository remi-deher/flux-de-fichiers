//! Connexion OAuth « device code » du fournisseur ChatGPT (même flux que Codex CLI, ADR 0015).
//!
//! Le central affiche un code, l'administrateur le valide sur la page du fournisseur depuis
//! n'importe quel appareil, puis le central échange ce code contre des jetons. Les jetons sont
//! chiffrés au repos (`settings`), rafraîchis automatiquement et jamais exposés au navigateur.

use super::{provider::pinned_client_for, settings};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    net_policy::OutboundPolicy,
    repository,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use forgearr_shared::ai::{
    AiOauthAccount, AiOauthDeviceCode, AiOauthPollResponse, AiOauthPollStatus,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{fmt, time::Duration};
use tokio::sync::Mutex;

/// Émetteur OAuth du fournisseur.
pub const OAUTH_ISSUER: &str = "https://auth.openai.com";
/// Identifiant public du client OAuth de Codex CLI (flux « Sign in with ChatGPT »).
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const USER_CODE_PATH: &str = "/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_PATH: &str = "/api/accounts/deviceauth/token";
const TOKEN_PATH: &str = "/oauth/token";
const DEVICE_REDIRECT_PATH: &str = "/deviceauth/callback";
const VERIFICATION_PATH: &str = "/codex/device";
const PENDING_KEY: &str = "ai_oauth_pending";
const DEVICE_CODE_TTL_SECONDS: u64 = 15 * 60;
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 5;
const REFRESH_MARGIN_SECONDS: i64 = 120;
const DEFAULT_ACCESS_TTL_SECONDS: i64 = 3600;
const OAUTH_TIMEOUT: Duration = Duration::from_secs(45);
const CLAIMS_AUTH: &str = "https://api.openai.com/auth";
const CLAIMS_PROFILE: &str = "https://api.openai.com/profile";

/// Sérialise les rafraîchissements : un jeton de refresh rotatif ne doit servir qu'une fois.
static REFRESH_LOCK: Mutex<()> = Mutex::const_new(());

/// Jetons OAuth déchiffrés, uniquement en mémoire du central.
#[derive(Clone, Serialize, Deserialize)]
pub struct OauthTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Expiration du jeton d'accès (secondes Unix).
    pub expires_at: i64,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

impl fmt::Debug for OauthTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OauthTokens")
            .field("expires_at", &self.expires_at)
            .field("account_id", &self.account_id)
            .finish_non_exhaustive()
    }
}

impl OauthTokens {
    pub fn account(&self) -> AiOauthAccount {
        AiOauthAccount {
            email: self.email.clone(),
            plan: self.plan.clone(),
        }
    }

    fn needs_refresh(&self, now: i64) -> bool {
        self.expires_at - now <= REFRESH_MARGIN_SECONDS
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PendingDevice {
    device_auth_id: String,
    user_code: String,
    interval_seconds: u64,
    expires_at: i64,
}

#[derive(Debug, Default, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Corps d'une requête OAuth : JSON (device code, refresh) ou formulaire (échange du code).
enum Payload {
    Json(Value),
    Form(Vec<(&'static str, String)>),
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

fn endpoint(issuer: &str, path: &str) -> String {
    format!("{}{path}", issuer.trim_end_matches('/'))
}

async fn post(
    issuer: &str,
    path: &str,
    payload: Payload,
    policy: OutboundPolicy,
) -> AppResult<(StatusCode, Value)> {
    let (url, client) = pinned_client_for(&endpoint(issuer, path), policy, OAUTH_TIMEOUT).await?;
    let request = client.post(url);
    let request = match payload {
        Payload::Json(body) => request.json(&body),
        Payload::Form(fields) => request.form(&fields),
    };
    let response = request.send().await.map_err(|err| {
        tracing::warn!(error = %err, "échec réseau OAuth IA");
        AppError::Internal
    })?;
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    Ok((status, body))
}

fn unexpected(status: StatusCode, context: &str) -> AppError {
    tracing::warn!(
        status = status.as_u16(),
        context,
        "réponse OAuth IA inattendue"
    );
    AppError::Internal
}

/// Lit un entier fourni soit en nombre, soit en chaîne (le fournisseur renvoie `"5"`).
fn lenient_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn text_field(body: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| body.get(*name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn claim_text(claims: &Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(claims, |node, key| node.get(*key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Assemble les jetons à persister ; l'ancien jeton de refresh et l'identité sont conservés
/// quand la réponse ne les renvoie pas.
fn build_tokens(
    response: TokenResponse,
    previous: Option<&OauthTokens>,
    now: i64,
) -> AppResult<OauthTokens> {
    let access_token = response.access_token.ok_or(AppError::Internal)?;
    let refresh_token = response
        .refresh_token
        .or_else(|| previous.map(|prev| prev.refresh_token.clone()))
        .ok_or(AppError::Internal)?;
    let expires_at = jwt_claims(&access_token)
        .and_then(|claims| claims.get("exp").and_then(Value::as_i64))
        .or_else(|| response.expires_in.map(|seconds| now + seconds))
        .unwrap_or(now + DEFAULT_ACCESS_TTL_SECONDS);
    let identity = response.id_token.as_deref().and_then(jwt_claims);
    let from_identity = |path: &[&str]| identity.as_ref().and_then(|c| claim_text(c, path));
    Ok(OauthTokens {
        access_token,
        refresh_token,
        expires_at,
        account_id: from_identity(&[CLAIMS_AUTH, "chatgpt_account_id"])
            .or_else(|| previous.and_then(|prev| prev.account_id.clone())),
        email: from_identity(&["email"])
            .or_else(|| from_identity(&[CLAIMS_PROFILE, "email"]))
            .or_else(|| previous.and_then(|prev| prev.email.clone())),
        plan: from_identity(&[CLAIMS_AUTH, "chatgpt_plan_type"])
            .or_else(|| previous.and_then(|prev| prev.plan.clone())),
    })
}

async fn read_pending(pool: &SqlitePool) -> Option<PendingDevice> {
    repository::get_setting(pool, PENDING_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

async fn write_pending(pool: &SqlitePool, pending: Option<&PendingDevice>) -> AppResult<()> {
    let encoded = match pending {
        Some(value) => serde_json::to_string(value).map_err(|_| AppError::Internal)?,
        None => String::new(),
    };
    repository::set_setting(pool, PENDING_KEY, &encoded).await
}

/// Démarre une connexion : demande un code utilisateur et le mémorise pour l'interrogation.
pub async fn start(
    pool: &SqlitePool,
    issuer: &str,
    policy: OutboundPolicy,
) -> AppResult<AiOauthDeviceCode> {
    let (status, body) = post(
        issuer,
        USER_CODE_PATH,
        Payload::Json(json!({ "client_id": CLIENT_ID })),
        policy,
    )
    .await?;
    if status == StatusCode::NOT_FOUND {
        // Connexion par code désactivée côté fournisseur.
        return Err(AppError::Conflict);
    }
    if !status.is_success() {
        return Err(unexpected(status, "device code"));
    }
    let device_auth_id =
        text_field(&body, &["device_auth_id"]).ok_or_else(|| unexpected(status, "device id"))?;
    let user_code = text_field(&body, &["user_code", "usercode"])
        .ok_or_else(|| unexpected(status, "user code"))?;
    let interval_seconds = lenient_u64(body.get("interval"))
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS);
    let pending = PendingDevice {
        device_auth_id,
        user_code: user_code.clone(),
        interval_seconds,
        expires_at: now_unix() + i64::try_from(DEVICE_CODE_TTL_SECONDS).unwrap_or(0),
    };
    write_pending(pool, Some(&pending)).await?;
    Ok(AiOauthDeviceCode {
        user_code,
        verification_url: endpoint(issuer, VERIFICATION_PATH),
        interval_seconds,
        expires_in_seconds: DEVICE_CODE_TTL_SECONDS,
    })
}

fn poll_state(status: AiOauthPollStatus, account: Option<AiOauthAccount>) -> AiOauthPollResponse {
    AiOauthPollResponse { status, account }
}

/// Interroge la connexion en cours ; enregistre les jetons dès que le code est validé.
pub async fn poll(
    pool: &SqlitePool,
    config: &Config,
    issuer: &str,
    policy: OutboundPolicy,
) -> AppResult<AiOauthPollResponse> {
    let Some(pending) = read_pending(pool).await else {
        return Ok(poll_state(AiOauthPollStatus::Expired, None));
    };
    if pending.expires_at <= now_unix() {
        write_pending(pool, None).await?;
        return Ok(poll_state(AiOauthPollStatus::Expired, None));
    }
    let (status, body) = post(
        issuer,
        DEVICE_TOKEN_PATH,
        Payload::Json(json!({
            "device_auth_id": pending.device_auth_id,
            "user_code": pending.user_code,
        })),
        policy,
    )
    .await?;
    if matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND) {
        return Ok(poll_state(AiOauthPollStatus::Pending, None));
    }
    if !status.is_success() {
        return Err(unexpected(status, "device token"));
    }
    let authorization_code = text_field(&body, &["authorization_code"])
        .ok_or_else(|| unexpected(status, "authorization code"))?;
    let code_verifier =
        text_field(&body, &["code_verifier"]).ok_or_else(|| unexpected(status, "verifier"))?;
    let (status, body) = post(
        issuer,
        TOKEN_PATH,
        Payload::Form(vec![
            ("grant_type", "authorization_code".to_owned()),
            ("client_id", CLIENT_ID.to_owned()),
            ("code", authorization_code),
            ("redirect_uri", endpoint(issuer, DEVICE_REDIRECT_PATH)),
            ("code_verifier", code_verifier),
        ]),
        policy,
    )
    .await?;
    if !status.is_success() {
        return Err(unexpected(status, "token exchange"));
    }
    let response: TokenResponse = serde_json::from_value(body).map_err(|_| AppError::Internal)?;
    let tokens = build_tokens(response, None, now_unix())?;
    settings::save_oauth_tokens(pool, config, Some(&tokens)).await?;
    write_pending(pool, None).await?;
    Ok(poll_state(
        AiOauthPollStatus::Connected,
        Some(tokens.account()),
    ))
}

/// Retourne des jetons valides, en rafraîchissant le jeton d'accès s'il est proche de l'expiration.
///
/// `Conflict` signale qu'aucune connexion n'existe ou qu'elle doit être refaite.
pub async fn fresh_tokens(
    pool: &SqlitePool,
    config: &Config,
    issuer: &str,
    policy: OutboundPolicy,
) -> AppResult<OauthTokens> {
    let current = settings::load_oauth_tokens(pool, config)
        .await
        .ok_or(AppError::Conflict)?;
    if !current.needs_refresh(now_unix()) {
        return Ok(current);
    }
    let _guard = REFRESH_LOCK.lock().await;
    // Un autre appel a pu rafraîchir pendant l'attente du verrou.
    let current = settings::load_oauth_tokens(pool, config)
        .await
        .ok_or(AppError::Conflict)?;
    if !current.needs_refresh(now_unix()) {
        return Ok(current);
    }
    let (status, body) = post(
        issuer,
        TOKEN_PATH,
        Payload::Json(json!({
            "grant_type": "refresh_token",
            "client_id": CLIENT_ID,
            "refresh_token": current.refresh_token,
        })),
        policy,
    )
    .await?;
    if matches!(status, StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED) {
        tracing::warn!(status = status.as_u16(), "connexion OAuth IA à refaire");
        return Err(AppError::Conflict);
    }
    if !status.is_success() {
        return Err(unexpected(status, "refresh"));
    }
    let response: TokenResponse = serde_json::from_value(body).map_err(|_| AppError::Internal)?;
    let refreshed = build_tokens(response, Some(&current), now_unix())?;
    settings::save_oauth_tokens(pool, config, Some(&refreshed)).await?;
    Ok(refreshed)
}

/// Oublie les jetons enregistrés et toute connexion en cours.
pub async fn disconnect(pool: &SqlitePool, config: &Config) -> AppResult<()> {
    settings::save_oauth_tokens(pool, config, None).await?;
    write_pending(pool, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::settings::tests_support::{test_config, test_pool};
    use axum::{Json, Router, routing::post as route_post};
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    const LOOPBACK: OutboundPolicy = OutboundPolicy {
        allow_private_hosts: true,
        require_https: false,
    };

    fn fake_jwt(claims: &Value) -> String {
        let encode = |value: &Value| URL_SAFE_NO_PAD.encode(value.to_string());
        format!(
            "{}.{}.sig",
            encode(&json!({ "alg": "none" })),
            encode(claims)
        )
    }

    fn identity_token() -> String {
        fake_jwt(&json!({
            "email": "remi@example.test",
            CLAIMS_AUTH: { "chatgpt_account_id": "acct-1", "chatgpt_plan_type": "plus" },
        }))
    }

    async fn spawn(router: Router) -> anyhow::Result<String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(format!("http://{addr}"))
    }

    #[test]
    fn build_tokens_reads_identity_expiry_and_keeps_previous_refresh() -> anyhow::Result<()> {
        let access = fake_jwt(&json!({ "exp": 2_000_000_000_i64 }));
        let first = build_tokens(
            TokenResponse {
                id_token: Some(identity_token()),
                access_token: Some(access),
                refresh_token: Some("r1".into()),
                expires_in: None,
            },
            None,
            1_000,
        )?;
        assert_eq!(first.expires_at, 2_000_000_000);
        assert_eq!(first.account_id.as_deref(), Some("acct-1"));
        assert_eq!(first.email.as_deref(), Some("remi@example.test"));
        assert_eq!(first.plan.as_deref(), Some("plus"));

        let renewed = build_tokens(
            TokenResponse {
                access_token: Some("opaque".into()),
                expires_in: Some(600),
                ..TokenResponse::default()
            },
            Some(&first),
            1_000,
        )?;
        assert_eq!(renewed.refresh_token, "r1");
        assert_eq!(renewed.expires_at, 1_600);
        assert_eq!(renewed.account_id.as_deref(), Some("acct-1"));

        assert!(build_tokens(TokenResponse::default(), None, 0).is_err());
        assert!(!format!("{first:?}").contains("r1"));
        Ok(())
    }

    #[test]
    fn needs_refresh_uses_the_safety_margin() {
        let tokens = OauthTokens {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 1_000,
            account_id: None,
            email: None,
            plan: None,
        };
        assert!(tokens.needs_refresh(1_000 - REFRESH_MARGIN_SECONDS));
        assert!(!tokens.needs_refresh(1_000 - REFRESH_MARGIN_SECONDS - 1));
    }

    #[test]
    fn lenient_integers_accept_numbers_and_strings() {
        assert_eq!(lenient_u64(Some(&json!(7))), Some(7));
        assert_eq!(lenient_u64(Some(&json!("5"))), Some(5));
        assert_eq!(lenient_u64(Some(&json!("x"))), None);
        assert_eq!(lenient_u64(None), None);
    }

    #[tokio::test]
    async fn device_flow_connects_refreshes_and_disconnects() -> anyhow::Result<()> {
        let polls = Arc::new(AtomicUsize::new(0));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let (poll_count, refresh_count) = (Arc::clone(&polls), Arc::clone(&refreshes));
        let identity = identity_token();
        let router = Router::new()
            .route(
                USER_CODE_PATH,
                route_post(|Json(body): Json<Value>| async move {
                    assert_eq!(body["client_id"], CLIENT_ID);
                    Json(json!({
                        "device_auth_id": "dev-1",
                        "user_code": "ABCD-1234",
                        "interval": "7",
                    }))
                }),
            )
            .route(
                DEVICE_TOKEN_PATH,
                route_post(move |Json(body): Json<Value>| {
                    let attempt = poll_count.fetch_add(1, Ordering::SeqCst);
                    async move {
                        assert_eq!(body["device_auth_id"], "dev-1");
                        if attempt == 0 {
                            return (StatusCode::FORBIDDEN, Json(json!({})));
                        }
                        (
                            StatusCode::OK,
                            Json(json!({
                                "authorization_code": "code-1",
                                "code_challenge": "c",
                                "code_verifier": "v",
                            })),
                        )
                    }
                }),
            )
            .route(
                TOKEN_PATH,
                route_post({
                    let identity = identity.clone();
                    move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                        let identity = identity.clone();
                        let refresh_count = Arc::clone(&refresh_count);
                        async move {
                            let is_json = headers
                                .get("content-type")
                                .and_then(|v| v.to_str().ok())
                                .is_some_and(|v| v.starts_with("application/json"));
                            if is_json {
                                let parsed: Value =
                                    serde_json::from_slice(&body).unwrap_or_default();
                                assert_eq!(parsed["grant_type"], "refresh_token");
                                assert_eq!(parsed["refresh_token"], "refresh-1");
                                refresh_count.fetch_add(1, Ordering::SeqCst);
                                return Json(json!({
                                    "access_token": "access-2",
                                    "refresh_token": "refresh-2",
                                    "expires_in": 3600,
                                }));
                            }
                            let form = form_pairs(&body);
                            assert_eq!(form["grant_type"], "authorization_code");
                            assert_eq!(form["code"], "code-1");
                            assert_eq!(form["code_verifier"], "v");
                            assert!(form["redirect_uri"].ends_with(DEVICE_REDIRECT_PATH));
                            Json(json!({
                                "id_token": identity,
                                "access_token": "access-1",
                                "refresh_token": "refresh-1",
                                "expires_in": 30,
                            }))
                        }
                    }
                }),
            );
        let issuer = spawn(router).await?;

        let dir = tempfile::tempdir()?;
        let pool = test_pool().await?;
        let config = test_config(dir.path());

        // Sans connexion : rien à rafraîchir, aucune connexion en cours.
        assert!(matches!(
            fresh_tokens(&pool, &config, &issuer, LOOPBACK).await,
            Err(AppError::Conflict)
        ));
        let idle = poll(&pool, &config, &issuer, LOOPBACK).await?;
        assert_eq!(idle.status, AiOauthPollStatus::Expired);

        let code = start(&pool, &issuer, LOOPBACK).await?;
        assert_eq!(code.user_code, "ABCD-1234");
        assert_eq!(code.interval_seconds, 7);
        assert_eq!(
            code.verification_url,
            format!("{issuer}{VERIFICATION_PATH}")
        );

        let waiting = poll(&pool, &config, &issuer, LOOPBACK).await?;
        assert_eq!(waiting.status, AiOauthPollStatus::Pending);
        let done = poll(&pool, &config, &issuer, LOOPBACK).await?;
        assert_eq!(done.status, AiOauthPollStatus::Connected);
        assert_eq!(
            done.account.and_then(|account| account.email).as_deref(),
            Some("remi@example.test")
        );
        // La connexion en cours est consommée.
        let after = poll(&pool, &config, &issuer, LOOPBACK).await?;
        assert_eq!(after.status, AiOauthPollStatus::Expired);

        // Jeton à 30 s de l'expiration : refresh, identité conservée, jeton rotatif enregistré.
        let renewed = fresh_tokens(&pool, &config, &issuer, LOOPBACK).await?;
        assert_eq!(renewed.access_token, "access-2");
        assert_eq!(renewed.account_id.as_deref(), Some("acct-1"));
        let reused = fresh_tokens(&pool, &config, &issuer, LOOPBACK).await?;
        assert_eq!(reused.access_token, "access-2");
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);

        let stored = repository::get_setting(&pool, settings::AI_SETTINGS_KEY)
            .await?
            .unwrap_or_default();
        assert!(!stored.contains("access-2") && !stored.contains("refresh-2"));

        disconnect(&pool, &config).await?;
        assert!(settings::load_oauth_tokens(&pool, &config).await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn start_maps_disabled_device_login_to_conflict() -> anyhow::Result<()> {
        let issuer = spawn(Router::new()).await?;
        let pool = test_pool().await?;
        assert!(matches!(
            start(&pool, &issuer, LOOPBACK).await,
            Err(AppError::Conflict)
        ));
        Ok(())
    }

    /// Décode un corps `application/x-www-form-urlencoded` simple (clés et valeurs sûres).
    fn form_pairs(body: &[u8]) -> HashMap<String, String> {
        reqwest::Url::parse(&format!("http://x/?{}", String::from_utf8_lossy(body)))
            .map(|url| url.query_pairs().into_owned().collect())
            .unwrap_or_default()
    }
}
