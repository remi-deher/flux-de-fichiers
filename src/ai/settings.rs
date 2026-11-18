//! Persistance et chiffrement au repos des réglages de l'assistant IA (`instance_settings`).

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
            allow_write_tools: self.allow_write_tools,
            max_tool_rounds: self.max_tool_rounds,
        }
    }

    pub fn as_client_status(&self) -> AiClientStatus {
        let ready = self.enabled
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
            && (!self.provider.requires_api_key()
                || self.api_key.as_ref().is_some_and(|k| !k.is_empty()));
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

fn decrypt_api_key(config: &Config, stored: Option<&str>) -> Option<String> {
    let text = stored?.trim();
    if text.is_empty() || !is_encrypted(text) {
        return None;
    }
    let secrets = SecretBox::open(&config.config_dir).ok()?;
    secrets.decrypt(text, AI_API_KEY_AAD).ok()
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
    let api_key = decrypt_api_key(config, persisted.api_key_enc.as_deref());
    let max_tool_rounds =
        clamp_rounds(persisted.max_tool_rounds.unwrap_or(DEFAULT_MAX_TOOL_ROUNDS));
    EffectiveAiSettings {
        enabled: persisted.enabled,
        provider,
        base_url,
        model,
        api_key,
        allow_write_tools: persisted.allow_write_tools,
        max_tool_rounds,
    }
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
        allow_write_tools: request.allow_write_tools,
        max_tool_rounds: Some(clamp_rounds(request.max_tool_rounds)),
    };
    let encoded = serde_json::to_string(&next).map_err(|_| AppError::Internal)?;
    repository::set_setting(pool, AI_SETTINGS_KEY, &encoded).await?;
    Ok(resolve_effective(config, &next))
}

/// Re-chiffre l'éventuelle clé d'API IA persistée lors d'une rotation de clé locale.
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
    let Some(ref enc) = persisted.api_key_enc else {
        return Ok(0);
    };
    if !is_encrypted(enc) {
        return Ok(0);
    }
    let clear = old.decrypt(enc, AI_API_KEY_AAD)?;
    persisted.api_key_enc = Some(new.encrypt(&clear, AI_API_KEY_AAD)?);
    let updated = serde_json::to_string(&persisted).map_err(|_| AppError::Internal)?;
    sqlx::query("UPDATE instance_settings SET value = ?1 WHERE key = ?2")
        .bind(updated)
        .bind(AI_SETTINGS_KEY)
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

    #[tokio::test]
    async fn ai_settings_encrypt_preserve_clear_and_rotate_api_key() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let pool = test_pool().await?;
        let config = base_config(dir.path());

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
}
