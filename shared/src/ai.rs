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
            // Aucun nom fixe : la liste des modèles dépend du compte (voir `has_automatic_model`).
            Self::ChatGptOauth => "",
        }
    }

    pub const fn requires_api_key(self) -> bool {
        matches!(self, Self::OpenAiCompatible | Self::Anthropic)
    }

    /// Un modèle vide est permis : le central choisit le premier modèle proposé au compte.
    pub const fn has_automatic_model(self) -> bool {
        matches!(self, Self::ChatGptOauth)
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

/// Modèle proposé par le fournisseur pour la connexion testée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelOption {
    /// Identifiant envoyé au fournisseur (`gpt-4o`, `qwen2.5:7b`…).
    pub id: String,
    /// Nom affiché ; l'identifiant quand le fournisseur n'en donne pas.
    pub label: String,
}

impl AiModelOption {
    pub fn new(id: impl Into<String>, label: Option<&str>) -> Self {
        let id = id.into();
        let label = label
            .map(str::trim)
            .filter(|text| !text.is_empty() && *text != id)
            .map_or_else(|| id.clone(), |text| format!("{text} · {id}"));
        Self { id, label }
    }
}

/// Résultat d'un test de connexion au fournisseur LLM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResponse {
    pub reachable: bool,
    pub latency_ms: u64,
    pub available_models: Vec<AiModelOption>,
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
    /// Écran et objet ouverts ; le serveur en déduit quoi lire par ses outils.
    #[serde(default)]
    pub context: Option<AiContext>,
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

/// Objet de l'interface sur lequel porte la conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiContextKind {
    Flow,
    Job,
    Library,
    Plugin,
    Worker,
}

impl AiContextKind {
    /// Nom de l'objet, tel que présenté au modèle.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flow => "flux",
            Self::Job => "tâche",
            Self::Library => "bibliothèque",
            Self::Plugin => "plugin",
            Self::Worker => "worker",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextObject {
    pub kind: AiContextKind,
    pub id: String,
}

/// Contexte typé joint à une question : l'écran ouvert et, s'il y en a un, l'objet affiché.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContext {
    /// Chemin de l'écran (`/app/flows`), sans contenu.
    pub screen: String,
    #[serde(default)]
    pub object: Option<AiContextObject>,
}

/// Réponse de `POST /api/v1/ai/runs` : conversation lancée en tâche de fond.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRunStarted {
    pub run_id: String,
}

/// Réponse de l'utilisateur à une demande de confirmation (`POST /api/v1/ai/runs/{id}/actions/{action}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiActionDecision {
    pub approve: bool,
}

/// Évènement publié par `GET /api/v1/ai/runs/{id}/events` (SSE, un évènement par message).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AiRunEvent {
    /// Un outil va être exécuté.
    ToolStarted { tool_name: String },
    /// Un outil a fini (succès, erreur ou refus).
    ToolFinished { trace: AiToolCallTrace },
    /// Une action d'écriture attend la réponse de l'utilisateur ; le run est suspendu.
    Confirmation {
        action_id: String,
        tool_name: String,
        /// Arguments de l'outil, secrets masqués.
        summary: String,
    },
    /// L'action a été autorisée ou refusée (ou le délai est écoulé : refus).
    ActionResolved { action_id: String, approved: bool },
    /// Réponse finale ; clôt le flux.
    Done { response: AiChatResponse },
    /// Échec de la conversation ; clôt le flux.
    Failed { message: String },
}

impl AiRunEvent {
    /// Dernier évènement d'un flux.
    pub const fn is_final(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Failed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_events_are_tagged_and_final_ones_close_the_stream() -> Result<(), serde_json::Error> {
        let started = AiRunEvent::ToolStarted {
            tool_name: "list_workers".into(),
        };
        assert_eq!(
            serde_json::to_value(&started)?,
            serde_json::json!({ "type": "tool_started", "toolName": "list_workers" })
        );
        assert!(!started.is_final());
        assert!(
            AiRunEvent::Failed {
                message: "x".into()
            }
            .is_final()
        );
        let context = AiContext {
            screen: "/app/flows".into(),
            object: Some(AiContextObject {
                kind: AiContextKind::Flow,
                id: "f1".into(),
            }),
        };
        let back: AiContext = serde_json::from_str(&serde_json::to_string(&context)?)?;
        assert_eq!(back, context);
        Ok(())
    }

    #[test]
    fn model_option_label_shows_the_name_only_when_it_adds_information() {
        assert_eq!(AiModelOption::new("gpt-5", None).label, "gpt-5");
        assert_eq!(AiModelOption::new("gpt-5", Some("gpt-5")).label, "gpt-5");
        assert_eq!(AiModelOption::new("gpt-5", Some("  ")).label, "gpt-5");
        assert_eq!(
            AiModelOption::new("gpt-5", Some("GPT-5")).label,
            "GPT-5 · gpt-5"
        );
    }

    #[test]
    fn providers_round_trip_and_expose_defaults() {
        for provider in AiProvider::ALL {
            assert_eq!(AiProvider::parse(provider.as_str()), Some(provider));
            assert!(!provider.label().is_empty());
            assert!(provider.default_base_url().starts_with("http"));
            assert_eq!(
                provider.default_model().is_empty(),
                provider.has_automatic_model()
            );
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
