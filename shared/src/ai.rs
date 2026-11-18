//! Contrats partagés de l'assistant IA intégré dans l'interface web (ADR 0015).

use serde::{Deserialize, Serialize};
use std::fmt;

pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_CHATGPT_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_MAX_TOOL_ROUNDS: u8 = 5;
pub const MIN_TOOL_ROUNDS: u8 = 1;
pub const MAX_TOOL_ROUNDS: u8 = 10;
pub const MAX_CHAT_MESSAGES: usize = 24;

/// Fournisseur de modèle de langage configuré sur le central.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    #[default]
    Ollama,
    OpenAiCompatible,
    Anthropic,
    /// Abonnement ChatGPT via connexion OAuth (flux device code, comme Codex CLI).
    ChatGptOauth,
}

impl AiProvider {
    pub const ALL: [Self; 4] = [
        Self::Ollama,
        Self::OpenAiCompatible,
        Self::Anthropic,
        Self::ChatGptOauth,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAiCompatible => "open_ai_compatible",
            Self::Anthropic => "anthropic",
            Self::ChatGptOauth => "chatgpt_oauth",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "ollama" => Some(Self::Ollama),
            "open_ai_compatible" | "openai_compatible" | "openai" => Some(Self::OpenAiCompatible),
            "anthropic" => Some(Self::Anthropic),
            "chatgpt_oauth" => Some(Self::ChatGptOauth),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama (local / LAN)",
            Self::OpenAiCompatible => "OpenAI / API compatible",
            Self::Anthropic => "Anthropic Claude",
            Self::ChatGptOauth => "ChatGPT (connexion OAuth, abonnement)",
        }
    }

    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::Ollama => DEFAULT_OLLAMA_URL,
            Self::OpenAiCompatible => DEFAULT_OPENAI_URL,
            Self::Anthropic => DEFAULT_ANTHROPIC_URL,
            Self::ChatGptOauth => DEFAULT_CHATGPT_URL,
        }
    }

    pub const fn default_model(self) -> &'static str {
        match self {
            Self::Ollama => "qwen2.5:7b",
            Self::OpenAiCompatible => "gpt-4o-mini",
            Self::Anthropic => "claude-3-5-sonnet-latest",
            Self::ChatGptOauth => "gpt-5-codex",
        }
    }

    pub const fn requires_api_key(self) -> bool {
        matches!(self, Self::OpenAiCompatible | Self::Anthropic)
    }

    /// Le fournisseur s'authentifie par une connexion OAuth plutôt que par une clé d'API.
    pub const fn uses_oauth(self) -> bool {
        matches!(self, Self::ChatGptOauth)
    }
}

/// Compte rattaché à une connexion OAuth (aucun jeton n'est jamais exposé).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOauthAccount {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

/// Code à saisir sur la page de vérification du fournisseur (`POST /api/v1/ai/oauth/start`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOauthDeviceCode {
    pub user_code: String,
    pub verification_url: String,
    pub interval_seconds: u64,
    pub expires_in_seconds: u64,
}

/// Avancement d'une connexion OAuth en cours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiOauthPollStatus {
    /// L'utilisateur n'a pas encore validé le code.
    Pending,
    /// Connexion terminée, jetons enregistrés.
    Connected,
    /// Le code a expiré ou aucune connexion n'est en cours.
    Expired,
}

/// Réponse d'interrogation de la connexion en cours (`POST /api/v1/ai/oauth/poll`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOauthPollResponse {
    pub status: AiOauthPollStatus,
    #[serde(default)]
    pub account: Option<AiOauthAccount>,
}

impl fmt::Display for AiProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration de l'assistant IA vue par les administrateurs (sans jamais renvoyer la clé).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub enabled: bool,
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    pub api_key_set: bool,
    /// Compte connecté par OAuth, s'il y en a un.
    #[serde(default)]
    pub oauth_account: Option<AiOauthAccount>,
    pub allow_write_tools: bool,
    pub max_tool_rounds: u8,
}

impl Default for AiSettingsView {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AiProvider::Ollama,
            base_url: DEFAULT_OLLAMA_URL.to_owned(),
            model: AiProvider::Ollama.default_model().to_owned(),
            api_key_set: false,
            oauth_account: None,
            allow_write_tools: false,
            max_tool_rounds: DEFAULT_MAX_TOOL_ROUNDS,
        }
    }
}

/// Mise à jour des réglages de l'assistant IA (`PUT /api/v1/ai/settings`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAiSettingsRequest {
    pub enabled: bool,
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    /// `None` conserve la clé enregistrée, `Some("")` l'efface, `Some(val)` la remplace.
    #[serde(default)]
    pub api_key: Option<String>,
    pub allow_write_tools: bool,
    pub max_tool_rounds: u8,
}

/// Requête de test de connectivité / détection de modèles (`POST /api/v1/ai/test`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTestRequest {
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Résultat d'un test de connexion au fournisseur LLM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResponse {
    pub reachable: bool,
    pub latency_ms: u64,
    pub available_models: Vec<String>,
    pub message: String,
}

/// État léger exposé à toute session authentifiée sur le central (`GET /api/v1/ai/status`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiClientStatus {
    pub available: bool,
    pub provider: AiProvider,
    pub model: String,
    pub allow_write_tools: bool,
}

/// Rôle d'un tour de conversation dans l'assistant web.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiChatRole {
    User,
    Assistant,
}

/// Message d'historique échangé avec `POST /api/v1/ai/chat`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessage {
    pub role: AiChatRole,
    pub content: String,
}

/// Trace concise d'un outil MCP invoqué pendant la réponse.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolCallTrace {
    pub tool_name: String,
    pub read_only: bool,
    pub outcome: String,
    pub duration_ms: u64,
}

/// Requête envoyée à l'assistant IA intégré (`POST /api/v1/ai/chat`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatRequest {
    pub messages: Vec<AiChatMessage>,
    #[serde(default)]
    pub context_hint: Option<String>,
}

/// Réponse complète de l'assistant après résolution éventuelle des appels d'outils.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatResponse {
    pub reply: String,
    pub model: String,
    pub provider: AiProvider,
    #[serde(default)]
    pub tool_calls: Vec<AiToolCallTrace>,
    pub rounds_used: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_round_trip_and_expose_defaults() {
        for provider in AiProvider::ALL {
            assert_eq!(AiProvider::parse(provider.as_str()), Some(provider));
            assert!(!provider.label().is_empty());
            assert!(provider.default_base_url().starts_with("http"));
            assert!(!provider.default_model().is_empty());
        }
        assert_eq!(
            AiProvider::parse("openai"),
            Some(AiProvider::OpenAiCompatible)
        );
        assert_eq!(AiProvider::parse("unknown"), None);
        assert!(!AiProvider::Ollama.requires_api_key());
        assert!(AiProvider::OpenAiCompatible.requires_api_key());
        assert!(AiProvider::Anthropic.requires_api_key());
        assert!(!AiProvider::ChatGptOauth.requires_api_key());
        assert!(AiProvider::ChatGptOauth.uses_oauth());
        assert!(!AiProvider::Anthropic.uses_oauth());
    }
}
