//! Canal des suggestions de code de l'éditeur de script (ADR 0032, lot b1).
//!
//! La page et le central échangent ces messages Forgearr typés sur un WebSocket. Le navigateur n'envoie jamais
//! de JSON-RPC brut : il ne peut donc demander au serveur de langage que ce que ces messages permettent.
//! Les positions suivent le protocole de langage : ligne depuis 0, colonne en unités UTF-16.

use crate::local::ScriptInterpreter;
use serde::{Deserialize, Serialize};

/// Adresse du WebSocket d'une session de langage.
pub const LANGUAGE_SESSION_PATH: &str = "/api/v1/language/session";
/// Paramètre de requête qui nomme l'interpréteur du script édité.
pub const INTERPRETER_QUERY_PARAMETER: &str = "interpreter";
/// Identifiant de l'application de catalogue qui livre les serveurs de langage.
pub const LANGUAGE_TOOL_ID: &str = "langages";
/// Au-delà, le canal est fermé : un message de la page n'a pas à dépasser le texte d'un script.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 1024 * 1024;
/// Nombre de suggestions rendues à la page au plus.
pub const MAX_SUGGESTIONS: usize = 200;

/// Serveur de langage qui répond pour un interpréteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanguageServer {
    /// `bash-language-server`, avec ShellCheck pour les diagnostics (bash et sh).
    Bash,
    /// `ty server` (Python).
    Python,
}

impl LanguageServer {
    /// Serveur d'un interpréteur ; `None` pour ceux qui restent sur la liste du client (node, deno, bun, pwsh).
    pub const fn for_interpreter(interpreter: ScriptInterpreter) -> Option<Self> {
        match interpreter {
            ScriptInterpreter::Bash | ScriptInterpreter::Sh => Some(Self::Bash),
            ScriptInterpreter::Python3 => Some(Self::Python),
            ScriptInterpreter::Node
            | ScriptInterpreter::Deno
            | ScriptInterpreter::Bun
            | ScriptInterpreter::Pwsh => None,
        }
    }

    /// Binaire du serveur, à la racine de l'archive de l'application `langages`.
    pub const fn binary(self) -> &'static str {
        match self {
            Self::Bash => "bash-language-server",
            Self::Python => "ty",
        }
    }

    /// Arguments qui lancent le serveur en mode LSP sur stdio.
    pub const fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::Bash => &["start"],
            Self::Python => &["server"],
        }
    }

    /// Extension du document ouvert : certains serveurs s'en servent pour reconnaître le langage.
    pub const fn file_extension(self) -> &'static str {
        match self {
            Self::Bash => "sh",
            Self::Python => "py",
        }
    }

    /// Identifiant de langage du protocole.
    pub const fn language_id(self) -> &'static str {
        match self {
            Self::Bash => "shellscript",
            Self::Python => "python",
        }
    }
}

/// Position dans le texte : ligne depuis 0, colonne en unités UTF-16.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// Étendue du texte, de `start` (inclus) à `end` (exclu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Gravité d'un diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Anomalie signalée par le serveur de langage.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Nature d'une suggestion, pour son pictogramme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionKind {
    Function,
    Variable,
    Keyword,
    Module,
    Property,
    Snippet,
    Other,
}

/// Suggestion rendue par le serveur de langage.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub label: String,
    pub kind: SuggestionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Texte inséré s'il diffère du libellé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
}

/// Pourquoi la session n'a pas de serveur de langage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnavailableReason {
    /// Aucun serveur de langage ne sert cet interpréteur.
    InterpreterUnsupported,
    /// L'application `langages` n'est pas installée ou son serveur ne démarre pas.
    ServerMissing,
    /// Le plafond de sessions ouvertes est atteint.
    LimitReached,
    /// Le serveur s'est arrêté (inactivité, panne, retrait de l'application).
    ServerStopped,
}

/// Messages de la page vers le central.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ClientMessage {
    /// Ouvre le document avec son texte entier.
    Open { text: String },
    /// Remplace le texte entier du document.
    Change { version: u32, text: String },
    /// Demande les suggestions à `position` ; `id` revient dans la réponse.
    Complete { id: u32, position: Position },
    /// Demande l'info-bulle à `position` ; `id` revient dans la réponse.
    Hover { id: u32, position: Position },
    /// Ferme la session et arrête le serveur.
    Close,
}

/// Messages du central vers la page.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    /// Le serveur de langage est prêt : le document peut être ouvert.
    Ready,
    /// Diagnostics du document, poussés dès que le serveur les publie (liste entière, remplaçante).
    Diagnostics { items: Vec<Diagnostic> },
    /// Réponse à `Complete` ; vide si le serveur n'a rien proposé ou n'a pas répondu à temps.
    Completion { id: u32, items: Vec<Suggestion> },
    /// Réponse à `Hover` ; sans texte s'il n'y a rien à dire à cet endroit.
    Hover { id: u32, text: Option<String> },
    /// La session n'a pas de serveur de langage : la page garde sa liste locale.
    Unavailable { reason: UnavailableReason },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bash_and_sh_share_a_server_and_python_has_its_own() {
        assert_eq!(
            LanguageServer::for_interpreter(ScriptInterpreter::Bash),
            Some(LanguageServer::Bash)
        );
        assert_eq!(
            LanguageServer::for_interpreter(ScriptInterpreter::Sh),
            Some(LanguageServer::Bash)
        );
        assert_eq!(
            LanguageServer::for_interpreter(ScriptInterpreter::Python3),
            Some(LanguageServer::Python)
        );
    }

    #[test]
    fn interpreters_without_a_server_stay_on_the_local_list() {
        for interpreter in [
            ScriptInterpreter::Node,
            ScriptInterpreter::Deno,
            ScriptInterpreter::Bun,
            ScriptInterpreter::Pwsh,
        ] {
            assert_eq!(LanguageServer::for_interpreter(interpreter), None);
        }
    }

    #[test]
    fn every_interpreter_is_decided() {
        let served = ScriptInterpreter::ALL
            .iter()
            .filter(|interpreter| LanguageServer::for_interpreter(**interpreter).is_some())
            .count();
        assert_eq!(served, 3);
    }

    #[test]
    fn a_server_names_its_binary_arguments_and_document() {
        assert_eq!(LanguageServer::Bash.binary(), "bash-language-server");
        assert_eq!(LanguageServer::Bash.arguments(), ["start"]);
        assert_eq!(LanguageServer::Python.binary(), "ty");
        assert_eq!(LanguageServer::Python.arguments(), ["server"]);
        assert_eq!(LanguageServer::Bash.file_extension(), "sh");
        assert_eq!(LanguageServer::Python.language_id(), "python");
    }

    #[test]
    fn client_messages_use_a_type_tag_and_camel_case_fields() {
        let message = ClientMessage::Change {
            version: 3,
            text: "echo".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&message).unwrap_or_default(),
            json!({"type": "change", "version": 3, "text": "echo"})
        );
        let parsed: Result<ClientMessage, _> = serde_json::from_value(json!({
            "type": "complete",
            "id": 7,
            "position": {"line": 2, "character": 5}
        }));
        assert_eq!(
            parsed.ok(),
            Some(ClientMessage::Complete {
                id: 7,
                position: Position {
                    line: 2,
                    character: 5,
                },
            })
        );
    }

    #[test]
    fn a_message_the_page_may_not_send_is_refused() {
        for raw in [
            json!({"type": "workspace/executeCommand"}),
            json!({"type": "shutdown"}),
            json!({"method": "initialize"}),
            json!({"type": "open"}),
        ] {
            assert!(serde_json::from_value::<ClientMessage>(raw).is_err());
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let messages = [
            ServerMessage::Ready,
            ServerMessage::Diagnostics {
                items: vec![Diagnostic {
                    range: Range::default(),
                    severity: DiagnosticSeverity::Warning,
                    message: "variable non définie".to_owned(),
                    source: Some("shellcheck".to_owned()),
                    code: Some("SC2154".to_owned()),
                }],
            },
            ServerMessage::Completion {
                id: 1,
                items: vec![Suggestion {
                    label: "dirname".to_owned(),
                    kind: SuggestionKind::Function,
                    detail: None,
                    insert_text: Some("dirname ".to_owned()),
                }],
            },
            ServerMessage::Hover {
                id: 2,
                text: Some("`echo`".to_owned()),
            },
            ServerMessage::Unavailable {
                reason: UnavailableReason::LimitReached,
            },
        ];
        for message in messages {
            let text = serde_json::to_string(&message).unwrap_or_default();
            let parsed = serde_json::from_str::<ServerMessage>(&text).ok();
            assert_eq!(parsed, Some(message));
        }
    }

    #[test]
    fn the_unavailable_reason_is_named_in_kebab_case() {
        let message = ServerMessage::Unavailable {
            reason: UnavailableReason::ServerMissing,
        };
        assert_eq!(
            serde_json::to_value(&message).unwrap_or_default(),
            json!({"type": "unavailable", "reason": "server-missing"})
        );
    }

    #[test]
    fn empty_optional_fields_are_left_out() {
        let suggestion = Suggestion {
            label: "x".to_owned(),
            kind: SuggestionKind::Other,
            detail: None,
            insert_text: None,
        };
        assert_eq!(
            serde_json::to_value(&suggestion).unwrap_or_default(),
            json!({"label": "x", "kind": "other"})
        );
    }
}
