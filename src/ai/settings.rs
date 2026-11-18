//! Persistance et chiffrement au repos des réglages de l'assistant IA (`instance_settings`).

use super::oauth::OauthTokens;
use crate::{
    config::Config,
    error::{AppError, AppResult},
    repository,
    secrets::{SecretBox, is_encrypted},
};
use flux_shared::ai::{
    AiClientStatus, AiProvider, AiSettingsView, DEFAULT_MAX_TOOL_ROUNDS, MAX_TOOL_ROUNDS,
    MIN_TOOL_ROUNDS, UpdateAiSettingsRequest,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const AI_SETTINGS_KEY: &str = "ai_assistant_settings";
const AI_API_KEY_AAD: &str = "system:ai:api_key";
const AI_OAUTH_AAD: &str = "system:ai:oauth";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PersistedAiSettings {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    provider: AiProvider,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    api_key_enc: Option<String>,
    /// Jetons OAuth sérialisés en JSON puis chiffrés.
    #[serde(default)]
    oauth_enc: Option<String>,
    #[serde(default)]
    allow_write_tools: bool,
    #[serde(default)]
    max_tool_rounds: Option<u8>,
}

/// Configuration effective résolue (avec clé d'API éventuellement déchiffrée en mémoire).
#[derive(Clone, Debug)]
pub struct EffectiveAiSettings {
    pub enabled: bool,
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub oauth: Option<OauthTokens>,
    pub allow_write_tools: bool,
    pub max_tool_rounds: u8,
}

impl EffectiveAiSettings {
    pub fn as_view(&self) -> AiSettingsView {
        AiSettingsView {
            enabled: self.enabled,
            provider: self.provider,
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_key_set: self.api_key.as_ref().is_some_and(|k| !k.is_empty()),
            oauth_account: self.oauth.as_ref().map(OauthTokens::account),
            allow_write_tools: self.allow_write_tools,
            max_tool_rounds: self.max_tool_rounds,
        }
    }

    pub fn as_client_status(&self) -> AiClientStatus {
        let ready = self.enabled
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
            && (!self.provider.requires_api_key()
                || self.api_key.as_ref().is_some_and(|k| !k.is_empty()))
            && (!self.provider.uses_oauth() || self.oauth.is_some());
        AiClientStatus {
            available: ready,
            provider: self.provider,
            model: self.model.clone(),
            allow_write_tools: self.allow_write_tools,
        }
    }
}

fn clean_opt(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn clamp_rounds(rounds: u8) -> u8 {
    rounds.clamp(MIN_TOOL_ROUNDS, MAX_TOOL_ROUNDS)
}

fn decrypt_secret(config: &Config, stored: Option<&str>, aad: &str) -> Option<String> {
    let text = stored?.trim();
    if text.is_empty() || !is_encrypted(text) {
        return None;
    }
    let secrets = SecretBox::open(&config.config_dir).ok()?;
    secrets.decrypt(text, aad).ok()
}

fn decrypt_oauth(config: &Config, stored: Option<&str>) -> Option<OauthTokens> {
    let json = decrypt_secret(config, stored, AI_OAUTH_AAD)?;
    serde_json::from_str(&json).ok()
}

async fn read_persisted(pool: &SqlitePool) -> PersistedAiSettings {
    repository::get_setting(pool, AI_SETTINGS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub async fn load_effective(pool: &SqlitePool, config: &Config) -> EffectiveAiSettings {
    let persisted = read_persisted(pool).await;
    resolve_effective(config, &persisted)
}

fn resolve_effective(config: &Config, persisted: &PersistedAiSettings) -> EffectiveAiSettings {
    let provider = persisted.provider;
    let base_url = clean_opt(persisted.base_url.as_deref())
        .unwrap_or_else(|| provider.default_base_url().to_owned());
    let model = clean_opt(persisted.model.as_deref())
        .unwrap_or_else(|| provider.default_model().to_owned());
    let api_key = decrypt_secret(config, persisted.api_key_enc.as_deref(), AI_API_KEY_AAD);
    let oauth = decrypt_oauth(config, persisted.oauth_enc.as_deref());
    let max_tool_rounds =
        clamp_rounds(persisted.max_tool_rounds.unwrap_or(DEFAULT_MAX_TOOL_ROUNDS));
    EffectiveAiSettings {
        enabled: persisted.enabled,
        provider,
        base_url,
        model,
        api_key,
        oauth,
        allow_write_tools: persisted.allow_write_tools,
        max_tool_rounds,
    }
}

async fn write_persisted(pool: &SqlitePool, persisted: &PersistedAiSettings) -> AppResult<()> {
    let encoded = serde_json::to_string(persisted).map_err(|_| AppError::Internal)?;
    repository::set_setting(pool, AI_SETTINGS_KEY, &encoded).await
}

/// Jetons OAuth enregistrés (déchiffrés en mémoire), s'il y en a.
pub async fn load_oauth_tokens(pool: &SqlitePool, config: &Config) -> Option<OauthTokens> {
    let persisted = read_persisted(pool).await;
    decrypt_oauth(config, persisted.oauth_enc.as_deref())
}

/// Chiffre et enregistre les jetons OAuth (`None` les efface) sans toucher au reste des réglages.
pub async fn save_oauth_tokens(
    pool: &SqlitePool,
    config: &Config,
    tokens: Option<&OauthTokens>,
) -> AppResult<()> {
    let mut persisted = read_persisted(pool).await;
    persisted.oauth_enc = match tokens {
        Some(value) => {
            let json = serde_json::to_string(value).map_err(|_| AppError::Internal)?;
            let secrets = SecretBox::open(&config.config_dir)?;
            Some(secrets.encrypt(&json, AI_OAUTH_AAD)?)
        }
        None => None,
    };
    write_persisted(pool, &persisted).await
}

pub fn validate_base_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err(AppError::Invalid);
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| AppError::Invalid)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::Invalid);
    }
    Ok(trimmed.to_owned())
}

/// Valide, chiffre l'éventuelle clé d'API et persiste les réglages IA.
pub async fn update(
    pool: &SqlitePool,
    config: &Config,
    request: UpdateAiSettingsRequest,
) -> AppResult<EffectiveAiSettings> {
    let base_url = validate_base_url(&request.base_url)?;
    let model = request.model.trim().to_owned();
    if model.is_empty() || model.chars().any(char::is_control) {
        return Err(AppError::Invalid);
    }

    let previous = read_persisted(pool).await;
    let api_key_enc = match request.api_key {
        Some(ref raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                let secrets = SecretBox::open(&config.config_dir)?;
                Some(secrets.encrypt(trimmed, AI_API_KEY_AAD)?)
            }
        }
        None => previous.api_key_enc,
    };

    let next = PersistedAiSettings {
        enabled: request.enabled,
        provider: request.provider,
        base_url: Some(base_url),
        model: Some(model),
        api_key_enc,
        oauth_enc: previous.oauth_enc,
        allow_write_tools: request.allow_write_tools,
        max_tool_rounds: Some(clamp_rounds(request.max_tool_rounds)),
    };
    write_persisted(pool, &next).await?;
    Ok(resolve_effective(config, &next))
}

/// Re-chiffre un secret persisté ; renvoie `true` s'il a été remplacé.
fn rotate_secret(
    stored: &mut Option<String>,
    old: &SecretBox,
    new: &SecretBox,
    aad: &str,
) -> AppResult<bool> {
    let Some(enc) = stored.as_deref().filter(|value| is_encrypted(value)) else {
        return Ok(false);
    };
    let clear = old.decrypt(enc, aad)?;
    *stored = Some(new.encrypt(&clear, aad)?);
    Ok(true)
}

/// Re-chiffre la clé d'API et les jetons OAuth IA persistés lors d'une rotation de clé locale.
pub async fn rotate_secrets_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    old: &SecretBox,
    new: &SecretBox,
) -> AppResult<usize> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = ?1")
            .bind(AI_SETTINGS_KEY)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(raw) = raw else {
        return Ok(0);
    };
    let mut persisted: PersistedAiSettings =
        serde_json::from_str(&raw).map_err(|_| AppError::Internal)?;
    let rotated = usize::from(rotate_secret(
        &mut persisted.api_key_enc,
        old,
        new,
        AI_API_KEY_AAD,
    )?) + usize::from(rotate_secret(
        &mut persisted.oauth_enc,
        old,
        new,
        AI_OAUTH_AAD,
    )?);
    if rotated == 0 {
        return Ok(0);
    }
    let updated = serde_json::to_string(&persisted).map_err(|_| AppError::Internal)?;
    sqlx::query("UPDATE instance_settings SET value = ?1 WHERE key = ?2")
        .bind(updated)
        .bind(AI_SETTINGS_KEY)
        .execute(&mut **tx)
        .await?;
    Ok(rotated)
}

#[cfg(test)]
pub(super) mod tests_support {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    pub async fn test_pool() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    pub fn test_config(dir: &std::path::Path) -> Config {
        Config {
            bind: ([127, 0, 0, 1], 3000).into(),
            database_url: "sqlite::memory:".into(),
            data_dir: dir.to_path_buf(),
            input_dir: dir.join("input"),
            config_dir: dir.to_path_buf(),
            tools_cache_dir: dir.join("tools"),
            plugins_dir: None,
            static_dir: "frontend/dist".into(),
            max_file_bytes: 2048,
            local_worker: false,
            web_origin: None,
            web_session_ttl_seconds: 28_800,
            tls: None,
            smtp: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{test_config, test_pool};
    use super::*;

    #[tokio::test]
    async fn ai_settings_encrypt_preserve_clear_and_rotate_api_key() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let pool = test_pool().await?;
        let config = test_config(dir.path());

        let saved = update(
            &pool,
            &config,
            UpdateAiSettingsRequest {
                enabled: true,
                provider: AiProvider::OpenAiCompatible,
                base_url: "https://api.openai.com/v1/".into(),
                model: "gpt-4o-mini".into(),
                api_key: Some("sk-top-secret-123".into()),
                allow_write_tools: false,
                max_tool_rounds: 25,
            },
        )
        .await?;
        assert!(saved.enabled);
        assert_eq!(saved.base_url, "https://api.openai.com/v1");
        assert_eq!(saved.max_tool_rounds, MAX_TOOL_ROUNDS);
        assert!(saved.as_view().api_key_set);
        assert_eq!(saved.api_key.as_deref(), Some("sk-top-secret-123"));

        let stored = repository::get_setting(&pool, AI_SETTINGS_KEY)
            .await?
            .unwrap_or_default();
        assert!(!stored.contains("sk-top-secret-123"));

        // `api_key: None` conserve la clé existante.
        let kept = update(
            &pool,
            &config,
            UpdateAiSettingsRequest {
                enabled: true,
                provider: AiProvider::OpenAiCompatible,
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o".into(),
                api_key: None,
                allow_write_tools: true,
                max_tool_rounds: 4,
            },
        )
        .await?;
        assert_eq!(kept.api_key.as_deref(), Some("sk-top-secret-123"));

        // Rotation de clé locale re-chiffre la clé stockée.
        let old = SecretBox::open(&config.config_dir)?;
        let new = SecretBox::from_key_hex(&SecretBox::generate_key_hex())?;
        let mut tx = pool.begin().await?;
        let count = rotate_secrets_in_tx(&mut tx, &old, &new).await?;
        tx.commit().await?;
        assert_eq!(count, 1);

        let persisted = read_persisted(&pool).await;
        let decrypted = new.decrypt(
            persisted.api_key_enc.as_deref().unwrap_or_default(),
            AI_API_KEY_AAD,
        )?;
        assert_eq!(decrypted, "sk-top-secret-123");

        // `api_key: Some("")` efface la clé enregistrée.
        let cleared = update(
            &pool,
            &config,
            UpdateAiSettingsRequest {
                enabled: false,
                provider: AiProvider::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                model: "qwen2.5:7b".into(),
                api_key: Some(String::new()),
                allow_write_tools: false,
                max_tool_rounds: 5,
            },
        )
        .await?;
        assert!(!cleared.as_view().api_key_set);
        assert_eq!(cleared.api_key, None);
        Ok(())
    }

    fn sample_tokens() -> OauthTokens {
        OauthTokens {
            access_token: "access-secret".into(),
            refresh_token: "refresh-secret".into(),
            expires_at: 4_000_000_000,
            account_id: Some("acct-1".into()),
            email: Some("remi@example.test".into()),
            plan: Some("plus".into()),
        }
    }

    #[tokio::test]
    async fn oauth_tokens_are_encrypted_survive_updates_gate_readiness_and_rotate()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let pool = test_pool().await?;
        let config = test_config(dir.path());
        let request = |provider: AiProvider| UpdateAiSettingsRequest {
            enabled: true,
            provider,
            base_url: provider.default_base_url().into(),
            model: provider.default_model().into(),
            api_key: None,
            allow_write_tools: false,
            max_tool_rounds: 3,
        };

        // Sans connexion, le fournisseur OAuth n'est pas prêt.
        let before = update(&pool, &config, request(AiProvider::ChatGptOauth)).await?;
        assert!(!before.as_client_status().available);
        assert!(before.as_view().oauth_account.is_none());

        save_oauth_tokens(&pool, &config, Some(&sample_tokens())).await?;
        let stored = repository::get_setting(&pool, AI_SETTINGS_KEY)
            .await?
            .unwrap_or_default();
        assert!(!stored.contains("access-secret") && !stored.contains("refresh-secret"));

        // Une mise à jour des réglages conserve la connexion.
        let after = update(&pool, &config, request(AiProvider::ChatGptOauth)).await?;
        assert!(after.as_client_status().available);
        assert_eq!(
            after
                .as_view()
                .oauth_account
                .and_then(|a| a.plan)
                .as_deref(),
            Some("plus")
        );
        let loaded = load_oauth_tokens(&pool, &config).await;
        assert_eq!(
            loaded.map(|t| t.access_token).as_deref(),
            Some("access-secret")
        );

        // La rotation de clé locale re-chiffre aussi les jetons.
        let old = SecretBox::open(&config.config_dir)?;
        let new = SecretBox::from_key_hex(&SecretBox::generate_key_hex())?;
        let mut tx = pool.begin().await?;
        assert_eq!(rotate_secrets_in_tx(&mut tx, &old, &new).await?, 1);
        tx.commit().await?;
        let persisted = read_persisted(&pool).await;
        let json = new.decrypt(
            persisted.oauth_enc.as_deref().unwrap_or_default(),
            AI_OAUTH_AAD,
        )?;
        assert!(json.contains("refresh-secret"));

        // Déconnexion : les jetons disparaissent, le reste des réglages demeure.
        save_oauth_tokens(&pool, &config, None).await?;
        assert!(read_persisted(&pool).await.oauth_enc.is_none());
        assert!(read_persisted(&pool).await.enabled);
        Ok(())
    }
}
