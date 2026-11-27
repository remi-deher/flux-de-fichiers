//! Suggestions du serveur de langage dans l'éditeur (ADR 0032, lot b2) : la part pure du canal. Positions du
//! protocole, caractères déclencheurs, adresse du canal, fusion avec la liste locale (a) et état du lien.

use super::suggestions::{self, MAX_SUGGESTIONS, Suggestion, SuggestionKind};
use crate::live::reconnect::retry_delay;
use forgearr_shared::{
    language::{
        self, INTERPRETER_QUERY_PARAMETER, LANGUAGE_SESSION_PATH, LanguageServer, Position,
        UnavailableReason,
    },
    local::ScriptInterpreter,
};

/// Millisecondes dans une seconde, pour comparer les délais d'attente à l'horloge du navigateur.
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
/// Extension des scripts Python et des scripts shell, qui ont chacun un caractère déclencheur.
const PYTHON_EXTENSION: &str = "py";
const SHELL_EXTENSION: &str = "sh";
const PYTHON_TRIGGER: char = '.';
const SHELL_TRIGGER: char = '$';

/// Un serveur de langage répond-il pour cet interpréteur ? Sinon l'éditeur garde sa liste locale.
pub fn is_served(interpreter: ScriptInterpreter) -> bool {
    LanguageServer::for_interpreter(interpreter).is_some()
}

/// Ce caractère tapé demande-t-il les suggestions sans attendre d'autres lettres ? `.` en Python, `$` en shell.
pub fn is_trigger(extension: &str, typed: char) -> bool {
    match extension {
        PYTHON_EXTENSION => typed == PYTHON_TRIGGER,
        SHELL_EXTENSION => typed == SHELL_TRIGGER,
        _ => false,
    }
}

/// Dernier caractère avant le curseur (unités UTF-16, comme le navigateur).
pub fn char_before(code: &str, caret: usize) -> Option<char> {
    let units: Vec<u16> = code.encode_utf16().collect();
    let caret = caret.min(units.len());
    String::from_utf16_lossy(&units[..caret]).chars().next_back()
}

/// Position du curseur pour le serveur : ligne depuis 0, colonne en unités UTF-16.
pub fn language_position(code: &str, caret: usize) -> Position {
    let units: Vec<u16> = code.encode_utf16().collect();
    let before = &units[..caret.min(units.len())];
    let newline = u16::from(b'\n');
    let line = before.iter().filter(|unit| **unit == newline).count();
    let line_start = before
        .iter()
        .rposition(|unit| *unit == newline)
        .map_or(0, |index| index + 1);
    Position {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        character: u32::try_from(before.len() - line_start).unwrap_or(u32::MAX),
    }
}

/// Adresse du canal : `wss` derrière du TLS, `ws` sinon ; l'interpréteur est écrit comme dans l'API.
pub fn session_url(secure: bool, host: &str, interpreter: ScriptInterpreter) -> Option<String> {
    let scheme = if secure { "wss" } else { "ws" };
    let name = serde_json::to_value(interpreter).ok()?;
    let name = name.as_str()?;
    Some(format!(
        "{scheme}://{host}{LANGUAGE_SESSION_PATH}?{INTERPRETER_QUERY_PARAMETER}={name}"
    ))
}

fn local_kind(kind: language::SuggestionKind) -> SuggestionKind {
    match kind {
        language::SuggestionKind::Function => SuggestionKind::Function,
        language::SuggestionKind::Variable => SuggestionKind::Variable,
        language::SuggestionKind::Keyword => SuggestionKind::Keyword,
        language::SuggestionKind::Module => SuggestionKind::Module,
        language::SuggestionKind::Property => SuggestionKind::Property,
        language::SuggestionKind::Snippet => SuggestionKind::Snippet,
        language::SuggestionKind::Other => SuggestionKind::Word,
    }
}

/// Suggestions du serveur, gardées si elles commencent par le mot tapé (comme le fait un éditeur : les
/// serveurs rendent tout ce qui est possible à cet endroit).
pub fn from_server(items: &[language::Suggestion], prefix: &str) -> Vec<Suggestion> {
    items
        .iter()
        .filter(|item| suggestions::starts_with_ignore_case(&item.label, prefix))
        .map(|item| Suggestion {
            label: item.label.clone(),
            kind: local_kind(item.kind),
            insert: item.insert_text.clone(),
        })
        .collect()
}

/// Liste affichée : d'abord le serveur, dans son ordre, puis ce que la liste locale ajoute ; sans doublon.
pub fn merge(server: Vec<Suggestion>, local: Vec<Suggestion>) -> Vec<Suggestion> {
    let mut merged: Vec<Suggestion> = Vec::new();
    for item in server.into_iter().chain(local) {
        if !merged.iter().any(|known| known.label == item.label) {
            merged.push(item);
        }
    }
    merged.truncate(MAX_SUGGESTIONS);
    merged
}

/// État du lien avec le serveur de langage. `now_ms` vient de l'horloge du navigateur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    /// Pas encore ouvert.
    Idle,
    /// Ouverture en cours, poignée de main avec le serveur comprise ; `failures` compte les échecs d'avant.
    Connecting { failures: u32 },
    /// Le serveur a répondu `ready` : les demandes sont possibles.
    Ready,
    /// Le canal s'est fermé ; une nouvelle tentative est permise à `retry_at_ms`.
    Dropped { failures: u32, retry_at_ms: f64 },
    /// Aucun serveur ne répond pour cet éditeur : la liste locale reste seule.
    Local,
}

impl Link {
    /// Faut-il ouvrir le canal maintenant ?
    pub fn may_connect(self, now_ms: f64) -> bool {
        match self {
            Self::Idle => true,
            Self::Dropped { retry_at_ms, .. } => now_ms >= retry_at_ms,
            Self::Connecting { .. } | Self::Ready | Self::Local => false,
        }
    }

    /// Le canal s'ouvre : les échecs déjà comptés ne sont oubliés qu'à `ready`.
    pub fn connecting(self) -> Self {
        Self::Connecting {
            failures: self.failures(),
        }
    }

    /// Les demandes de suggestions sont-elles possibles ?
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    fn failures(self) -> u32 {
        match self {
            Self::Connecting { failures } | Self::Dropped { failures, .. } => failures,
            Self::Idle | Self::Ready | Self::Local => 0,
        }
    }

    /// Le canal s'est fermé ou a échoué : nouvelle tentative après un délai qui double à chaque échec.
    /// Un lien déjà `Local` le reste.
    pub fn dropped(self, now_ms: f64) -> Self {
        if self == Self::Local {
            return self;
        }
        let failures = self.failures();
        let wait_ms = retry_delay(failures).as_secs_f64() * MILLISECONDS_PER_SECOND;
        Self::Dropped {
            failures: failures.saturating_add(1),
            retry_at_ms: now_ms + wait_ms,
        }
    }

    /// Le central a annoncé qu'aucun serveur ne répond : un interpréteur ou une application absente ne se
    /// règle pas en réessayant ; un plafond atteint ou un serveur arrêté, si.
    pub fn unavailable(self, reason: UnavailableReason, now_ms: f64) -> Self {
        match reason {
            UnavailableReason::InterpreterUnsupported | UnavailableReason::ServerMissing => {
                Self::Local
            }
            UnavailableReason::LimitReached | UnavailableReason::ServerStopped => {
                self.dropped(now_ms)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::language::SuggestionKind as ServerKind;
    use std::time::Duration;

    fn server_item(label: &str, kind: ServerKind, insert: Option<&str>) -> language::Suggestion {
        language::Suggestion {
            label: label.to_owned(),
            kind,
            detail: None,
            insert_text: insert.map(str::to_owned),
        }
    }

    #[test]
    fn only_interpreters_with_a_server_are_served() {
        assert!(is_served(ScriptInterpreter::Bash));
        assert!(is_served(ScriptInterpreter::Sh));
        assert!(is_served(ScriptInterpreter::Python3));
        assert!(!is_served(ScriptInterpreter::Node));
        assert!(!is_served(ScriptInterpreter::Pwsh));
    }

    #[test]
    fn the_trigger_character_depends_on_the_language() {
        assert!(is_trigger("py", '.'));
        assert!(!is_trigger("py", '$'));
        assert!(is_trigger("sh", '$'));
        assert!(!is_trigger("sh", '.'));
        assert!(!is_trigger("js", '.'));
    }

    #[test]
    fn the_character_before_the_caret_counts_utf16_units() {
        assert_eq!(char_before("os.", 3), Some('.'));
        assert_eq!(char_before("a😀", 3), Some('😀'));
        assert_eq!(char_before("abc", 0), None);
        assert_eq!(char_before("abc", 99), Some('c'));
    }

    #[test]
    fn positions_are_lines_from_zero_and_columns_in_utf16_units() {
        let origin = Position {
            line: 0,
            character: 0,
        };
        assert_eq!(language_position("abc", 0), origin);
        let third = Position {
            line: 1,
            character: 3,
        };
        assert_eq!(language_position("echo\nabc", 8), third);
        // « 😀 » vaut 2 unités UTF-16, comme dans le protocole.
        let after_emoji = Position {
            line: 1,
            character: 3,
        };
        assert_eq!(language_position("x\n😀a", 5), after_emoji);
        let past_the_end = Position {
            line: 0,
            character: 2,
        };
        assert_eq!(language_position("ab", 99), past_the_end);
    }

    #[test]
    fn the_channel_address_follows_the_page_protocol_and_names_the_interpreter() {
        let python = session_url(false, "localhost:3000", ScriptInterpreter::Python3);
        assert_eq!(
            python.as_deref(),
            Some("ws://localhost:3000/api/v1/language/session?interpreter=python3")
        );
        let bash = session_url(true, "flux.example", ScriptInterpreter::Bash);
        assert_eq!(
            bash.as_deref(),
            Some("wss://flux.example/api/v1/language/session?interpreter=bash")
        );
    }

    #[test]
    fn server_suggestions_keep_their_text_and_match_the_typed_word() {
        let items = [
            server_item("dirname", ServerKind::Function, Some("dirname ")),
            server_item("echo", ServerKind::Function, None),
            server_item("DIRS", ServerKind::Variable, None),
        ];
        let kept = from_server(&items, "dir");
        let labels: Vec<&str> = kept.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["dirname", "DIRS"]);
        assert_eq!(kept[0].kind, SuggestionKind::Function);
        assert_eq!(kept[0].inserted(), "dirname ");
        assert_eq!(kept[1].kind, SuggestionKind::Variable);
        assert_eq!(from_server(&items, "").len(), items.len());
    }

    #[test]
    fn every_server_kind_has_a_local_one() {
        let pairs = [
            (ServerKind::Function, SuggestionKind::Function),
            (ServerKind::Variable, SuggestionKind::Variable),
            (ServerKind::Keyword, SuggestionKind::Keyword),
            (ServerKind::Module, SuggestionKind::Module),
            (ServerKind::Property, SuggestionKind::Property),
            (ServerKind::Snippet, SuggestionKind::Snippet),
            (ServerKind::Other, SuggestionKind::Word),
        ];
        for (server, local) in pairs {
            assert_eq!(local_kind(server), local);
        }
    }

    #[test]
    fn the_server_comes_first_and_the_local_list_completes_without_duplicates() {
        let server = vec![
            Suggestion::local("workdir", SuggestionKind::Function),
            Suggestion::local("workflow", SuggestionKind::Function),
        ];
        let local = vec![
            Suggestion::local("workdir", SuggestionKind::Word),
            Suggestion::local("workingFile", SuggestionKind::Contract),
        ];
        let merged = merge(server, local);
        let labels: Vec<&str> = merged.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["workdir", "workflow", "workingFile"]);
        assert_eq!(merged[0].kind, SuggestionKind::Function);
    }

    #[test]
    fn the_merged_list_is_capped() {
        let many: Vec<Suggestion> = (0..MAX_SUGGESTIONS * 2)
            .map(|index| Suggestion::local(&format!("word{index}"), SuggestionKind::Word))
            .collect();
        assert_eq!(merge(many, Vec::new()).len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn a_new_link_may_connect_and_only_a_ready_one_answers() {
        assert!(Link::Idle.may_connect(0.0));
        assert!(!Link::Idle.connecting().may_connect(0.0));
        assert!(!Link::Ready.may_connect(0.0));
        assert!(Link::Ready.is_ready());
        assert!(!Link::Idle.connecting().is_ready());
        assert!(!Link::Local.may_connect(f64::MAX));
    }

    #[test]
    fn a_dropped_link_waits_longer_after_each_failure() {
        let first = Link::Ready.dropped(1000.0);
        let Link::Dropped {
            failures: 1,
            retry_at_ms: first_at,
        } = first
        else {
            panic!("lien inattendu : {first:?}");
        };
        assert!(!first.may_connect(first_at - 1.0));
        assert!(first.may_connect(first_at));
        let second = first.dropped(first_at);
        let Link::Dropped {
            failures: 2,
            retry_at_ms: second_at,
        } = second
        else {
            panic!("lien inattendu : {second:?}");
        };
        assert!(second_at - first_at > first_at - 1000.0);
    }

    #[test]
    fn failures_are_kept_while_reconnecting_and_forgotten_once_ready() {
        let dropped = Link::Idle.dropped(0.0).dropped(10.0);
        assert!(matches!(dropped, Link::Dropped { failures: 2, .. }));
        let retrying = dropped.connecting();
        assert_eq!(retrying, Link::Connecting { failures: 2 });
        let again = retrying.dropped(20.0);
        assert!(matches!(again, Link::Dropped { failures: 3, .. }));
        assert_eq!(Link::Ready.connecting(), Link::Connecting { failures: 0 });
    }

    #[test]
    fn the_wait_never_exceeds_the_shared_ceiling() {
        let mut link = Link::Idle;
        let mut now = 0.0;
        for _ in 0..40 {
            link = link.dropped(now);
            if let Link::Dropped { retry_at_ms, .. } = link {
                let ceiling = Duration::from_secs(60).as_secs_f64() * MILLISECONDS_PER_SECOND;
                assert!(retry_at_ms - now <= ceiling);
                now = retry_at_ms;
            }
        }
    }

    #[test]
    fn a_missing_server_is_final_and_a_busy_or_stopped_one_is_retried() {
        let now = 5000.0;
        for reason in [
            UnavailableReason::InterpreterUnsupported,
            UnavailableReason::ServerMissing,
        ] {
            assert_eq!(Link::Idle.connecting().unavailable(reason, now), Link::Local);
        }
        for reason in [
            UnavailableReason::LimitReached,
            UnavailableReason::ServerStopped,
        ] {
            let link = Link::Ready.unavailable(reason, now);
            assert!(matches!(link, Link::Dropped { failures: 1, .. }));
        }
    }

    #[test]
    fn a_local_link_stays_local_when_the_channel_closes() {
        assert_eq!(Link::Local.dropped(1.0), Link::Local);
    }
}
