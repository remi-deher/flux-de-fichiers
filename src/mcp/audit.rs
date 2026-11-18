//! Limitation de débit par clé et préparation sûre du journal d'audit MCP (ADR 0015).

use crate::{
    auth::Principal,
    error::AppError,
    repository::{self, NewMcpAuditEntry},
    tool_run::mask_log_secrets,
};
use flux_shared::mcp::McpAuditOutcome;
use serde_json::Value;
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, VecDeque},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use super::protocol::McpToolResult;

/// Fenêtre glissante par clé d'API pour les appels MCP (`POST /mcp`).
pub const MCP_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
/// Nombre maximal de requêtes `POST /mcp` autorisées par clé sur `MCP_RATE_LIMIT_WINDOW`.
pub const MCP_RATE_LIMIT_MAX: usize = 120;
const MAX_TRACKED_KEYS: usize = 1024;
const MAX_TARGET_SUMMARY_CHARS: usize = 160;
const MAX_ERROR_MESSAGE_CHARS: usize = 240;

const SAFE_TARGET_KEYS: &[&str] = &[
    "id",
    "flowId",
    "libraryId",
    "pluginId",
    "toolId",
    "packId",
    "jobId",
    "workerId",
    "name",
    "sourceName",
    "version",
    "uri",
    "action",
];

/// Limiteur à fenêtre glissante par clé (utilisé en production via l'instance globale et en test).
pub struct SlidingRateLimiter {
    max_requests: usize,
    window: Duration,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl SlidingRateLimiter {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Vérifie si `key` peut effectuer un appel à l'instant `now`.
    pub fn check_at(&self, key: &str, now: Instant) -> Result<(), AppError> {
        let Ok(mut guard) = self.hits.lock() else {
            return Ok(());
        };
        if guard.len() >= MAX_TRACKED_KEYS {
            let cutoff = now.checked_sub(self.window).unwrap_or(now);
            guard.retain(|_, queue| {
                while queue.front().is_some_and(|ts| *ts < cutoff) {
                    queue.pop_front();
                }
                !queue.is_empty()
            });
            if guard.len() >= MAX_TRACKED_KEYS {
                guard.clear();
            }
        }
        let queue = guard.entry(key.to_owned()).or_default();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while queue.front().is_some_and(|ts| *ts < cutoff) {
            queue.pop_front();
        }
        if queue.len() >= self.max_requests {
            return Err(AppError::RateLimited);
        }
        queue.push_back(now);
        Ok(())
    }
}

static MCP_RATE_LIMITER: LazyLock<SlidingRateLimiter> =
    LazyLock::new(|| SlidingRateLimiter::new(MCP_RATE_LIMIT_MAX, MCP_RATE_LIMIT_WINDOW));

pub fn check_mcp_rate_limit(key_id: &str) -> Result<(), AppError> {
    MCP_RATE_LIMITER.check_at(key_id, Instant::now())
}

fn truncate_chars(raw: &str, max_chars: usize) -> String {
    let mut iter = raw.chars();
    let collected: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}

/// Construit un résumé court et non sensible de la cible d'un outil MCP.
pub fn summarize_target(args: &Value) -> Option<String> {
    let obj = args.as_object()?;
    let mut parts = Vec::new();
    for key in SAFE_TARGET_KEYS {
        let Some(value) = obj.get(*key) else {
            continue;
        };
        let formatted = match value {
            Value::String(text) if !text.trim().is_empty() => text.trim().to_owned(),
            Value::Number(num) => num.to_string(),
            _ => continue,
        };
        if *key == "id" || *key == "uri" {
            parts.push(formatted);
        } else {
            parts.push(format!("{key}={formatted}"));
        }
        if parts.len() >= 3 {
            break;
        }
    }
    if parts.is_empty() {
        return None;
    }
    let joined = mask_log_secrets(&parts.join(", "));
    Some(truncate_chars(&joined, MAX_TARGET_SUMMARY_CHARS))
}

/// Extrait un message d'erreur borné et nettoyé de tout secret éventuel.
pub fn summarize_error_text(raw: &str) -> Option<String> {
    let first_line = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .trim();
    if first_line.is_empty() {
        return None;
    }
    let masked = mask_log_secrets(first_line);
    Some(truncate_chars(&masked, MAX_ERROR_MESSAGE_CHARS))
}

pub fn summarize_tool_error(result: &McpToolResult) -> Option<String> {
    if !result.is_error {
        return None;
    }
    let text = result
        .content
        .first()
        .map(|part| part.text.as_str())
        .unwrap_or("");
    summarize_error_text(text)
}

/// Enregistre un événement MCP en journalisation best-effort afin de ne pas bloquer le protocole.
#[allow(clippy::too_many_arguments)]
pub async fn log_event(
    pool: &SqlitePool,
    principal: &Principal,
    tool_name: &str,
    target_summary: Option<&str>,
    outcome: McpAuditOutcome,
    error_message: Option<&str>,
    duration_ms: i64,
) {
    let entry = NewMcpAuditEntry {
        key_id: &principal.id,
        key_name: &principal.name,
        role: principal.role.as_str(),
        tool_name,
        target_summary,
        outcome,
        error_message,
        duration_ms,
    };
    if let Err(err) = repository::record_mcp_audit(pool, entry).await {
        tracing::warn!(
            error = %err,
            tool = tool_name,
            principal_id = %principal.id,
            "impossible d'enregistrer l'entrée d'audit MCP"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarize_target_ignores_secrets_and_extracts_identifiers() {
        let args = json!({
            "id": "plugin.radarr",
            "settings": {
                "apiKey": "super-secret-value",
                "password": "hunter2"
            },
            "custom_headers": {
                "Authorization": "Bearer secret-token"
            },
            "version": 4
        });
        let summary = summarize_target(&args).unwrap_or_default();
        assert!(summary.contains("plugin.radarr"));
        assert!(summary.contains("version=4"));
        assert!(!summary.contains("super-secret-value"));
        assert!(!summary.contains("hunter2"));
        assert!(!summary.contains("secret-token"));
    }

    #[test]
    fn summarize_error_masks_credentials_and_caps_length() {
        let raw =
            "Connexion refusée sur https://user:secretpass@radarr.local/api?apikey=abc123\nLigne 2";
        let summary = summarize_error_text(raw).unwrap_or_default();
        assert!(!summary.contains("secretpass"));
        assert!(!summary.contains("Ligne 2"));
    }

    #[test]
    fn rate_limiter_blocks_over_threshold_and_recovers_after_window() {
        let limiter = SlidingRateLimiter::new(3, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(limiter.check_at("key-a", t0).is_ok());
        assert!(
            limiter
                .check_at("key-a", t0 + Duration::from_secs(1))
                .is_ok()
        );
        assert!(
            limiter
                .check_at("key-a", t0 + Duration::from_secs(2))
                .is_ok()
        );
        assert!(matches!(
            limiter.check_at("key-a", t0 + Duration::from_secs(3)),
            Err(AppError::RateLimited)
        ));
        // Une autre clé reste indépendante.
        assert!(
            limiter
                .check_at("key-b", t0 + Duration::from_secs(3))
                .is_ok()
        );
        // Une fois la fenêtre expirée, l'accès reprend.
        assert!(
            limiter
                .check_at("key-a", t0 + Duration::from_secs(61))
                .is_ok()
        );
    }
}
