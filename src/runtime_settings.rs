//! Réglages réseau, session, SMTP et politique sortante modifiables à chaud en base (Lots B & C).
//! Les réglages de démarrage (`Config`) servent de valeurs par défaut tant qu'aucune surcharge
//! n'est enregistrée dans `instance_settings`. Le mot de passe SMTP est toujours chiffré au repos.

use crate::{
    config::{
        Config, MAX_WEB_SESSION_TTL_SECONDS, MIN_WEB_SESSION_TTL_SECONDS, SmtpConfig, SmtpTls,
        parse_canonical_web_origin,
    },
    error::{AppError, AppResult},
    net_policy::OutboundPolicy,
    plugins::SyncPolicy,
    repository,
    secrets::{SecretBox, is_encrypted},
};
use forgearr_shared::{
    jobs::{DEFAULT_UNREACHABLE_TIMEOUT_SECONDS, valid_unreachable_timeout},
    system::{OutboundPolicyStatus, SmtpStatus, UpdateSystemSettingsRequest},
    timezone::{DEFAULT_TIMEZONE, canonical_timezone},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::env;

pub const RUNTIME_SETTINGS_KEY: &str = "runtime_network_settings";
const SMTP_PASSWORD_AAD: &str = "system:smtp:password";
const DEFAULT_PLUGIN_ALLOW_PRIVATE_ENV: &str = crate::net_policy::ALLOW_PRIVATE_HOSTS_ENV;
const DEFAULT_INTEGRATION_BLOCK_PRIVATE_ENV: &str = crate::net_policy::OUTBOUND_BLOCK_PRIVATE_ENV;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PersistedSmtp {
    host: String,
    port: u16,
    tls: SmtpTls,
    username: Option<String>,
    password_enc: Option<String>,
    from: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PersistedRuntimeSettings {
    #[serde(default)]
    web_origin_set: bool,
    #[serde(default)]
    web_origin: Option<String>,
    #[serde(default)]
    session_ttl_seconds: Option<i64>,
    #[serde(default)]
    smtp_set: bool,
    #[serde(default)]
    smtp: Option<PersistedSmtp>,
    #[serde(default)]
    allow_private_plugin_hosts: Option<bool>,
    #[serde(default)]
    block_private_integration_hosts: Option<bool>,
    #[serde(default)]
    check_updates_enabled: Option<bool>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    unreachable_timeout_seconds: Option<i64>,
}

impl PersistedRuntimeSettings {
    fn unreachable_timeout(&self) -> i64 {
        self.unreachable_timeout_seconds
            .filter(|seconds| valid_unreachable_timeout(*seconds))
            .unwrap_or(DEFAULT_UNREACHABLE_TIMEOUT_SECONDS)
    }
}

/// Vue effective résolue (base SQLite si présente, sinon configuration de démarrage).
#[derive(Clone)]
pub struct EffectiveRuntimeSettings {
    pub config: Config,
    pub smtp_status: SmtpStatus,
    pub outbound: OutboundPolicyStatus,
    pub check_updates_enabled: bool,
    pub timezone: String,
    pub unreachable_timeout_seconds: i64,
}

impl EffectiveRuntimeSettings {
    pub fn sync_policy(&self) -> SyncPolicy {
        SyncPolicy {
            allow_private_hosts: self.outbound.allow_private_plugin_hosts,
            require_immutable_releases: SyncPolicy::from_env().require_immutable_releases,
        }
    }

    pub fn integration_policy(&self) -> OutboundPolicy {
        OutboundPolicy {
            allow_private_hosts: !self.outbound.block_private_integration_hosts,
            require_https: false,
        }
    }
}

fn default_smtp_status(config: &Config) -> SmtpStatus {
    match &config.smtp {
        Some(smtp) => SmtpStatus {
            configured: true,
            host: Some(smtp.host.clone()),
            port: smtp.port,
            tls: smtp.tls,
            username: smtp.username.clone(),
            from: Some(smtp.from.clone()),
            password_set: smtp.password.is_some(),
        },
        None => SmtpStatus {
            configured: false,
            host: None,
            port: SmtpTls::StartTls.default_port(),
            tls: SmtpTls::StartTls,
            username: None,
            from: None,
            password_set: false,
        },
    }
}

async fn read_persisted(pool: &SqlitePool) -> PersistedRuntimeSettings {
    repository::get_setting(pool, RUNTIME_SETTINGS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn decrypt_smtp_password(config: &Config, stored: Option<&str>) -> Option<String> {
    let text = stored?.trim();
    if text.is_empty() {
        return None;
    }
    if !is_encrypted(text) {
        return None;
    }
    let secrets = SecretBox::open(&config.config_dir).ok()?;
    secrets.decrypt(text, SMTP_PASSWORD_AAD).ok()
}

fn default_outbound_from_env() -> OutboundPolicyStatus {
    OutboundPolicyStatus {
        allow_private_plugin_hosts: env::var(DEFAULT_PLUGIN_ALLOW_PRIVATE_ENV)
            .is_ok_and(|value| value == "1"),
        block_private_integration_hosts: env::var(DEFAULT_INTEGRATION_BLOCK_PRIVATE_ENV)
            .is_ok_and(|value| value == "1"),
    }
}

fn default_timezone_from_env() -> String {
    env::var("TZ")
        .ok()
        .and_then(|raw| canonical_timezone(&raw))
        .unwrap_or_else(|| DEFAULT_TIMEZONE.to_owned())
}

/// Charge uniquement le fuseau horaire effectif (base SQLite, sinon `TZ`, sinon `UTC`).
pub async fn load_timezone(pool: &SqlitePool) -> String {
    let persisted = read_persisted(pool).await;
    persisted
        .timezone
        .as_deref()
        .and_then(canonical_timezone)
        .unwrap_or_else(default_timezone_from_env)
}

/// Délai d'injoignabilité (secondes) avant qu'une tâche ne soit reprise à son titulaire (ADR 0027).
pub async fn load_unreachable_timeout(pool: &SqlitePool) -> i64 {
    read_persisted(pool).await.unreachable_timeout()
}

/// Charge les réglages effectifs (surcharges SQLite appliquées par-dessus `base`).
pub async fn load_effective(pool: &SqlitePool, base: &Config) -> EffectiveRuntimeSettings {
    let persisted = read_persisted(pool).await;
    resolve_effective(base, &persisted)
}

fn resolve_effective(
    base: &Config,
    persisted: &PersistedRuntimeSettings,
) -> EffectiveRuntimeSettings {
    let mut effective = base.clone();
    if persisted.web_origin_set {
        effective.web_origin = persisted.web_origin.clone();
    }
    if let Some(ttl) = persisted.session_ttl_seconds
        && (MIN_WEB_SESSION_TTL_SECONDS..=MAX_WEB_SESSION_TTL_SECONDS).contains(&ttl)
    {
        effective.web_session_ttl_seconds = ttl;
    }
    if persisted.smtp_set {
        effective.smtp = persisted.smtp.as_ref().map(|smtp| {
            let password = decrypt_smtp_password(base, smtp.password_enc.as_deref());
            SmtpConfig {
                host: smtp.host.clone(),
                port: smtp.port,
                tls: smtp.tls,
                username: smtp.username.clone(),
                password,
                from: smtp.from.clone(),
            }
        });
    }

    let env_outbound = default_outbound_from_env();
    let outbound = OutboundPolicyStatus {
        allow_private_plugin_hosts: persisted
            .allow_private_plugin_hosts
            .unwrap_or(env_outbound.allow_private_plugin_hosts),
        block_private_integration_hosts: persisted
            .block_private_integration_hosts
            .unwrap_or(env_outbound.block_private_integration_hosts),
    };
    let timezone = persisted
        .timezone
        .as_deref()
        .and_then(canonical_timezone)
        .unwrap_or_else(default_timezone_from_env);
    let smtp_status = default_smtp_status(&effective);
    EffectiveRuntimeSettings {
        config: effective,
        smtp_status,
        outbound,
        check_updates_enabled: persisted.check_updates_enabled.unwrap_or(true),
        timezone,
        unreachable_timeout_seconds: persisted.unreachable_timeout(),
    }
}

fn clean_opt(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Valide, chiffre le mot de passe SMTP si fourni et persiste les réglages d'exécution.
pub async fn update(
    pool: &SqlitePool,
    base: &Config,
    request: UpdateSystemSettingsRequest,
) -> AppResult<EffectiveRuntimeSettings> {
    let web_origin = match clean_opt(request.web_origin) {
        Some(raw) => Some(parse_canonical_web_origin(&raw).map_err(|_| AppError::Invalid)?),
        None => None,
    };
    if !(MIN_WEB_SESSION_TTL_SECONDS..=MAX_WEB_SESSION_TTL_SECONDS)
        .contains(&request.session_ttl_seconds)
    {
        return Err(AppError::Invalid);
    }

    if request
        .unreachable_timeout_seconds
        .is_some_and(|seconds| !valid_unreachable_timeout(seconds))
    {
        return Err(AppError::Invalid);
    }

    let previous = read_persisted(pool).await;
    let smtp = if request.smtp_enabled {
        let host = clean_opt(request.smtp_host).ok_or(AppError::Invalid)?;
        if host.chars().any(char::is_control) {
            return Err(AppError::Invalid);
        }
        let from = clean_opt(request.smtp_from).ok_or(AppError::Invalid)?;
        let _: lettre::message::Mailbox = from.parse().map_err(|_| AppError::Invalid)?;
        let port = request
            .smtp_port
            .unwrap_or_else(|| request.smtp_tls.default_port());
        if port == 0 {
            return Err(AppError::Invalid);
        }
        let username = clean_opt(request.smtp_username);
        let password_enc = match username.as_ref() {
            None => None,
            Some(_) => match request.smtp_password {
                Some(ref raw) if !raw.is_empty() => {
                    let secrets = SecretBox::open(&base.config_dir)?;
                    Some(secrets.encrypt(raw, SMTP_PASSWORD_AAD)?)
                }
                Some(_) => None,
                None => previous.smtp.and_then(|s| s.password_enc),
            },
        };
        if username.is_some() && password_enc.is_none() {
            return Err(AppError::Invalid);
        }
        Some(PersistedSmtp {
            host,
            port,
            tls: request.smtp_tls,
            username,
            password_enc,
            from,
        })
    } else {
        None
    };

    let timezone = match clean_opt(request.timezone) {
        Some(raw) => Some(canonical_timezone(&raw).ok_or(AppError::Invalid)?),
        None => previous.timezone,
    };

    let next = PersistedRuntimeSettings {
        web_origin_set: true,
        web_origin,
        session_ttl_seconds: Some(request.session_ttl_seconds),
        smtp_set: true,
        smtp,
        allow_private_plugin_hosts: Some(request.allow_private_plugin_hosts),
        block_private_integration_hosts: Some(request.block_private_integration_hosts),
        check_updates_enabled: Some(request.check_updates_enabled),
        timezone,
        unreachable_timeout_seconds: request
            .unreachable_timeout_seconds
            .or(previous.unreachable_timeout_seconds),
    };
    let encoded = serde_json::to_string(&next).map_err(|_| AppError::Internal)?;
    repository::set_setting(pool, RUNTIME_SETTINGS_KEY, &encoded).await?;
    Ok(resolve_effective(base, &next))
}

/// Valide et enregistre uniquement le fuseau horaire IANA du central.
pub async fn update_timezone(
    pool: &SqlitePool,
    base: &Config,
    raw_timezone: &str,
) -> AppResult<EffectiveRuntimeSettings> {
    let canonical = canonical_timezone(raw_timezone).ok_or(AppError::Invalid)?;
    let mut persisted = read_persisted(pool).await;
    persisted.timezone = Some(canonical);
    let encoded = serde_json::to_string(&persisted).map_err(|_| AppError::Internal)?;
    repository::set_setting(pool, RUNTIME_SETTINGS_KEY, &encoded).await?;
    Ok(resolve_effective(base, &persisted))
}

/// Re-chiffre l'éventuel mot de passe SMTP persisté dans `instance_settings` lors d'une rotation de clé.
pub async fn rotate_secrets_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    old: &SecretBox,
    new: &SecretBox,
) -> AppResult<usize> {
    let raw: Option<String> = sqlx::query_scalar!(
        "SELECT value FROM instance_settings WHERE key = ?1",
        RUNTIME_SETTINGS_KEY
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(raw) = raw else {
        return Ok(0);
    };
    let mut persisted: PersistedRuntimeSettings =
        serde_json::from_str(&raw).map_err(|_| AppError::Internal)?;
    let Some(ref mut smtp) = persisted.smtp else {
        return Ok(0);
    };
    let Some(ref enc) = smtp.password_enc else {
        return Ok(0);
    };
    let clear = old.decrypt(enc, SMTP_PASSWORD_AAD)?;
    smtp.password_enc = Some(new.encrypt(&clear, SMTP_PASSWORD_AAD)?);
    let updated = serde_json::to_string(&persisted).map_err(|_| AppError::Internal)?;
    sqlx::query!(
        "UPDATE instance_settings SET value = ?1 WHERE key = ?2",
        updated,
        RUNTIME_SETTINGS_KEY
    )
    .execute(&mut **tx)
    .await?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    fn base_config(dir: &std::path::Path) -> Config {
        Config {
            bind: ([127, 0, 0, 1], 3000).into(),
            database_url: "sqlite::memory:".into(),
            data_dir: dir.to_path_buf(),
            input_dir: dir.join("input"),
            config_dir: dir.join("config"),
            tools_cache_dir: dir.join("tools"),
            plugins_dir: None,
            detected_plugins_dir: None,
            static_dir: "frontend/dist".into(),
            max_file_bytes: 2048,
            local_worker: false,
            web_origin: None,
            web_session_ttl_seconds: 28_800,
            tls: None,
            smtp: None,
        }
    }

    #[tokio::test]
    async fn updates_and_encrypts_runtime_settings_in_sqlite() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let base = base_config(dir.path());
        let pool = test_pool().await?;

        let updated = update(
            &pool,
            &base,
            UpdateSystemSettingsRequest {
                web_origin: Some("https://flux.exemple.fr".into()),
                session_ttl_seconds: 7200,
                smtp_enabled: true,
                smtp_host: Some("smtp.exemple.fr".into()),
                smtp_port: Some(587),
                smtp_tls: SmtpTls::StartTls,
                smtp_username: Some("mailer".into()),
                smtp_password: Some("super-secret-pass".into()),
                smtp_from: Some("Flux <noreply@exemple.fr>".into()),
                allow_private_plugin_hosts: true,
                block_private_integration_hosts: true,
                check_updates_enabled: true,
                timezone: Some("Europe/Paris".into()),
                unreachable_timeout_seconds: Some(120),
            },
        )
        .await?;

        assert_eq!(
            updated.config.web_origin.as_deref(),
            Some("https://flux.exemple.fr")
        );
        assert_eq!(updated.config.web_session_ttl_seconds, 7200);
        assert!(updated.smtp_status.configured);
        assert!(updated.smtp_status.password_set);
        assert!(updated.outbound.allow_private_plugin_hosts);
        assert!(updated.outbound.block_private_integration_hosts);

        // Le mot de passe en base est chiffré, jamais en clair.
        let stored = repository::get_setting(&pool, RUNTIME_SETTINGS_KEY)
            .await?
            .ok_or_else(|| anyhow::anyhow!("réglages attendus"))?;
        assert!(!stored.contains("super-secret-pass"));

        // Une mise à jour ultérieure avec `smtp_password: None` conserve le mot de passe existant.
        let kept = update(
            &pool,
            &base,
            UpdateSystemSettingsRequest {
                web_origin: Some("https://flux.exemple.fr".into()),
                session_ttl_seconds: 3600,
                smtp_enabled: true,
                smtp_host: Some("smtp.exemple.fr".into()),
                smtp_port: None,
                smtp_tls: SmtpTls::Tls,
                smtp_username: Some("mailer".into()),
                smtp_password: None,
                smtp_from: Some("Flux <noreply@exemple.fr>".into()),
                allow_private_plugin_hosts: false,
                block_private_integration_hosts: false,
                check_updates_enabled: false,
                timezone: None,
                unreachable_timeout_seconds: None,
            },
        )
        .await?;
        assert_eq!(kept.config.web_session_ttl_seconds, 3600);
        assert_eq!(kept.unreachable_timeout_seconds, 120);
        assert_eq!(load_unreachable_timeout(&pool).await, 120);
        assert_eq!(kept.smtp_status.port, 465);
        assert_eq!(
            kept.config
                .smtp
                .as_ref()
                .and_then(|s| s.password.as_deref()),
            Some("super-secret-pass")
        );
        Ok(())
    }

    #[tokio::test]
    async fn rejects_invalid_origin_ttl_or_incomplete_smtp() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let base = base_config(dir.path());
        let pool = test_pool().await?;

        assert!(matches!(
            update(
                &pool,
                &base,
                UpdateSystemSettingsRequest {
                    web_origin: Some("https://flux.exemple.fr/sous-chemin".into()),
                    session_ttl_seconds: 3600,
                    ..Default::default()
                },
            )
            .await,
            Err(AppError::Invalid)
        ));

        assert!(matches!(
            update(
                &pool,
                &base,
                UpdateSystemSettingsRequest {
                    session_ttl_seconds: 3600,
                    unreachable_timeout_seconds: Some(1),
                    ..Default::default()
                },
            )
            .await,
            Err(AppError::Invalid)
        ));

        assert!(matches!(
            update(
                &pool,
                &base,
                UpdateSystemSettingsRequest {
                    session_ttl_seconds: 10,
                    ..Default::default()
                },
            )
            .await,
            Err(AppError::Invalid)
        ));

        assert!(matches!(
            update(
                &pool,
                &base,
                UpdateSystemSettingsRequest {
                    session_ttl_seconds: 3600,
                    smtp_enabled: true,
                    smtp_host: Some("smtp.exemple.fr".into()),
                    smtp_from: Some("adresse-invalide".into()),
                    ..Default::default()
                },
            )
            .await,
            Err(AppError::Invalid)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn updates_and_validates_timezone_in_sqlite() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let base = base_config(dir.path());
        let pool = test_pool().await?;

        let updated = update_timezone(&pool, &base, "Europe/Paris").await?;
        assert_eq!(updated.timezone, "Europe/Paris");
        assert_eq!(load_timezone(&pool).await, "Europe/Paris");

        let gmt = update_timezone(&pool, &base, "GMT").await?;
        assert_eq!(gmt.timezone, "Etc/GMT");
        assert_eq!(load_timezone(&pool).await, "Etc/GMT");

        assert!(matches!(
            update_timezone(&pool, &base, "Invalid/Zone").await,
            Err(AppError::Invalid)
        ));
        assert_eq!(load_timezone(&pool).await, "Etc/GMT");
        Ok(())
    }
}
