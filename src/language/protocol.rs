//! Messages du protocole de langage que le central écrit et lit pour une session (ADR 0032, point 4).
//!
//! Seules les méthodes dont la session a besoin existent ici : `initialize`, `didOpen`, `didChange`
//! (texte entier), `completion`, `hover`, les diagnostics à la demande et `shutdown`. Jamais
//! `workspace/executeCommand`.

use forgearr_shared::language::{
    Diagnostic, DiagnosticSeverity, LanguageServer, MAX_SUGGESTIONS, Position, Range, Suggestion,
    SuggestionKind,
};
use serde_json::{Value, json};
use uuid::Uuid;

const JSONRPC_VERSION: &str = "2.0";
/// Dossier fictif des documents : rien n'existe à cet endroit, le texte vient du `didOpen`.
const DOCUMENT_ROOT: &str = "file:///forgearr";
/// Longueur maximale (en caractères) d'un libellé, d'un détail ou d'un texte inséré.
const MAX_FIELD_CHARS: usize = 500;
/// Longueur maximale (en caractères) d'une info-bulle.
const MAX_HOVER_CHARS: usize = 4000;
/// Nombre maximal de diagnostics rendus à la page.
const MAX_DIAGNOSTICS: usize = 500;
/// Codes des imports introuvables, masqués en Python : les paquets vivent sur les workers, pas sur le
/// central (ADR 0032, point 6).
const UNRESOLVED_IMPORT_CODES: &[&str] = &[
    "unresolved-import",
    "reportMissingImports",
    "reportMissingModuleSource",
];

pub const INITIALIZE: &str = "initialize";
pub const INITIALIZED: &str = "initialized";
pub const DID_OPEN: &str = "textDocument/didOpen";
pub const DID_CHANGE: &str = "textDocument/didChange";
pub const COMPLETION: &str = "textDocument/completion";
pub const HOVER: &str = "textDocument/hover";
pub const PULL_DIAGNOSTICS: &str = "textDocument/diagnostic";
pub const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
pub const SHUTDOWN: &str = "shutdown";
pub const EXIT: &str = "exit";
const WORKSPACE_CONFIGURATION: &str = "workspace/configuration";

/// Requête numérotée.
pub fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": JSONRPC_VERSION, "id": id, "method": method, "params": params})
}

/// Notification, sans réponse attendue.
pub fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": JSONRPC_VERSION, "method": method, "params": params})
}

/// Réponse à une requête du serveur.
fn response(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": JSONRPC_VERSION, "id": id, "result": result})
}

/// Adresse du document d'une session : `file:///forgearr/<session>/script.<extension>`.
pub fn document_uri(session: Uuid, server: LanguageServer) -> String {
    format!(
        "{DOCUMENT_ROOT}/{session}/script.{}",
        server.file_extension()
    )
}

/// Dossier racine de l'espace de travail d'une session.
pub fn workspace_uri(session: Uuid) -> String {
    format!("{DOCUMENT_ROOT}/{session}")
}

pub fn initialize_params(process_id: u32, session: Uuid) -> Value {
    json!({
        "processId": process_id,
        "rootUri": workspace_uri(session),
        "capabilities": {
            "textDocument": {
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "documentationFormat": ["markdown", "plaintext"],
                    }
                },
                "hover": {"contentFormat": ["markdown", "plaintext"]},
                "publishDiagnostics": {},
                "diagnostic": {"dynamicRegistration": false},
            },
            "workspace": {"configuration": true},
        },
    })
}

pub fn did_open_params(uri: &str, server: LanguageServer, text: &str) -> Value {
    json!({"textDocument": {
        "uri": uri,
        "languageId": server.language_id(),
        "version": 1,
        "text": text,
    }})
}

pub fn did_change_params(uri: &str, version: u32, text: &str) -> Value {
    json!({
        "textDocument": {"uri": uri, "version": version},
        "contentChanges": [{"text": text}],
    })
}

pub fn position_params(uri: &str, position: Position) -> Value {
    json!({
        "textDocument": {"uri": uri},
        "position": {"line": position.line, "character": position.character},
    })
}

pub fn pull_diagnostics_params(uri: &str) -> Value {
    json!({"textDocument": {"uri": uri}})
}

/// Réponse à une requête que le serveur adresse au client. Les réglages demandés sont vides (un `null`
/// fait échouer `ty`) ; le reste est accepté sans effet.
pub fn answer_to_server_request(id: &Value, method: &str, params: &Value) -> Value {
    if method == WORKSPACE_CONFIGURATION {
        let asked = params
            .get("items")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        return response(id, Value::Array(vec![json!({}); asked]));
    }
    response(id, Value::Null)
}

fn truncated(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn suggestion_kind(code: Option<u64>) -> SuggestionKind {
    match code {
        Some(2..=4) => SuggestionKind::Function,
        Some(5 | 10) => SuggestionKind::Property,
        Some(6 | 12 | 21) => SuggestionKind::Variable,
        Some(9) => SuggestionKind::Module,
        Some(14) => SuggestionKind::Keyword,
        Some(15) => SuggestionKind::Snippet,
        _ => SuggestionKind::Other,
    }
}

fn suggestion_from(item: &Value) -> Option<Suggestion> {
    let label = truncated(item.get("label")?.as_str()?, MAX_FIELD_CHARS);
    let text_of = |field: &str| {
        item
            .get(field)
            .and_then(Value::as_str)
            .map(|text| truncated(text, MAX_FIELD_CHARS))
            .filter(|text| !text.is_empty())
    };
    let insert_text = text_of("insertText").filter(|text| *text != label);
    Some(Suggestion {
        kind: suggestion_kind(item.get("kind").and_then(Value::as_u64)),
        detail: text_of("detail"),
        insert_text,
        label,
    })
}

/// Suggestions d'un résultat de `completion` (liste, `CompletionList` ou `null`), au plus `MAX_SUGGESTIONS`.
pub fn suggestions_from(result: &Value) -> Vec<Suggestion> {
    let items = result.get("items").unwrap_or(result);
    let Some(items) = items.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(suggestion_from)
        .take(MAX_SUGGESTIONS)
        .collect()
}

fn markup_text(part: &Value) -> Option<&str> {
    part
        .as_str()
        .or_else(|| part.get("value").and_then(Value::as_str))
}

/// Texte d'une info-bulle (`MarkupContent`, `MarkedString`, liste des deux), `None` s'il est vide.
pub fn hover_text_from(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    let parts: Vec<&str> = match contents.as_array() {
        Some(list) => list.iter().filter_map(markup_text).collect(),
        None => markup_text(contents).into_iter().collect(),
    };
    let text = parts.join("\n\n");
    let text = text.trim();
    (!text.is_empty()).then(|| truncated(text, MAX_HOVER_CHARS))
}

fn position_from(value: &Value) -> Option<Position> {
    let coordinate = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_u64)
            .and_then(|number| u32::try_from(number).ok())
    };
    Some(Position {
        line: coordinate("line")?,
        character: coordinate("character")?,
    })
}

fn severity_from(code: Option<u64>) -> DiagnosticSeverity {
    match code {
        Some(2) => DiagnosticSeverity::Warning,
        Some(3) => DiagnosticSeverity::Information,
        Some(4) => DiagnosticSeverity::Hint,
        _ => DiagnosticSeverity::Error,
    }
}

fn code_from(value: &Value) -> Option<String> {
    match value.get("code")? {
        Value::String(code) => Some(code.clone()),
        Value::Number(code) => Some(code.to_string()),
        _ => None,
    }
}

fn diagnostic_from(item: &Value) -> Option<Diagnostic> {
    let range = item.get("range")?;
    Some(Diagnostic {
        range: Range {
            start: position_from(range.get("start")?)?,
            end: position_from(range.get("end")?)?,
        },
        severity: severity_from(item.get("severity").and_then(Value::as_u64)),
        message: truncated(item.get("message")?.as_str()?, MAX_FIELD_CHARS),
        source: item
            .get("source")
            .and_then(Value::as_str)
            .map(|source| truncated(source, MAX_FIELD_CHARS)),
        code: code_from(item),
    })
}

fn is_unresolved_import(diagnostic: &Diagnostic) -> bool {
    diagnostic
        .code
        .as_deref()
        .is_some_and(|code| UNRESOLVED_IMPORT_CODES.contains(&code))
}

/// Diagnostics d'une liste du serveur, sans les imports introuvables en Python, au plus `MAX_DIAGNOSTICS`.
pub fn diagnostics_from(items: &Value, server: LanguageServer) -> Vec<Diagnostic> {
    let Some(items) = items.as_array() else {
        return Vec::new();
    };
    let hides_imports = server == LanguageServer::Python;
    items
        .iter()
        .filter_map(diagnostic_from)
        .filter(|diagnostic| !(hides_imports && is_unresolved_import(diagnostic)))
        .take(MAX_DIAGNOSTICS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_notifications_follow_the_wire_format() {
        let asked = request(4, COMPLETION, json!({"a": 1}));
        assert_eq!(asked["jsonrpc"], "2.0");
        assert_eq!(asked["id"], 4);
        assert_eq!(asked["method"], COMPLETION);
        let told = notification(EXIT, Value::Null);
        assert!(told.get("id").is_none());
        assert_eq!(told["method"], EXIT);
    }

    #[test]
    fn the_document_lives_under_a_per_session_fictional_folder() {
        let session = Uuid::nil();
        assert_eq!(
            document_uri(session, LanguageServer::Python),
            format!("file:///forgearr/{session}/script.py")
        );
        assert_eq!(
            document_uri(session, LanguageServer::Bash),
            format!("file:///forgearr/{session}/script.sh")
        );
        assert_eq!(
            workspace_uri(session),
            format!("file:///forgearr/{session}")
        );
    }

    #[test]
    fn initialization_asks_for_diagnostics_completion_and_hover_only() {
        let params = initialize_params(42, Uuid::nil());
        assert_eq!(params["processId"], 42);
        let text_document = &params["capabilities"]["textDocument"];
        assert!(text_document.get("completion").is_some());
        assert!(text_document.get("hover").is_some());
        assert!(text_document.get("publishDiagnostics").is_some());
        assert_eq!(text_document["diagnostic"]["dynamicRegistration"], false);
    }

    #[test]
    fn opening_and_changing_send_the_whole_text() {
        let opened = did_open_params("file:///x", LanguageServer::Bash, "echo");
        assert_eq!(opened["textDocument"]["languageId"], "shellscript");
        assert_eq!(opened["textDocument"]["text"], "echo");
        let changed = did_change_params("file:///x", 3, "echo 1");
        assert_eq!(changed["textDocument"]["version"], 3);
        assert_eq!(changed["contentChanges"][0]["text"], "echo 1");
    }

    #[test]
    fn a_position_request_carries_the_uri_line_and_character() {
        let params = position_params(
            "file:///x",
            Position {
                line: 2,
                character: 5,
            },
        );
        assert_eq!(params["textDocument"]["uri"], "file:///x");
        assert_eq!(params["position"], json!({"line": 2, "character": 5}));
    }

    #[test]
    fn the_server_gets_one_empty_setting_per_item_asked() {
        let params = json!({"items": [{"section": "ty"}, {"section": "python"}]});
        let answer = answer_to_server_request(&json!(9), "workspace/configuration", &params);
        assert_eq!(answer["id"], 9);
        assert_eq!(answer["result"], json!([{}, {}]));
    }

    #[test]
    fn other_requests_of_the_server_are_accepted_without_effect() {
        let method = "client/registerCapability";
        let answer = answer_to_server_request(&json!("a"), method, &json!({}));
        assert_eq!(answer["id"], "a");
        assert_eq!(answer["result"], Value::Null);
    }

    #[test]
    fn suggestions_come_from_a_list_or_a_completion_list() {
        let item = json!({"label": "echo", "kind": 3, "detail": "builtin"});
        let completion_list = json!({"isIncomplete": false, "items": [item]});
        let from_list = suggestions_from(&json!([item]));
        let from_completion_list = suggestions_from(&completion_list);
        assert_eq!(from_list, from_completion_list);
        assert_eq!(from_list.len(), 1);
        assert_eq!(from_list[0].label, "echo");
        assert_eq!(from_list[0].kind, SuggestionKind::Function);
        assert_eq!(from_list[0].detail.as_deref(), Some("builtin"));
    }

    #[test]
    fn a_missing_or_null_result_gives_no_suggestion() {
        assert!(suggestions_from(&Value::Null).is_empty());
        assert!(suggestions_from(&json!({})).is_empty());
        assert!(suggestions_from(&json!([{"kind": 3}])).is_empty());
    }

    #[test]
    fn the_inserted_text_is_kept_only_when_it_differs_from_the_label() {
        let same = suggestions_from(&json!([{"label": "a", "insertText": "a"}]));
        let other = suggestions_from(&json!([{"label": "a", "insertText": "a()"}]));
        assert_eq!(same[0].insert_text, None);
        assert_eq!(other[0].insert_text.as_deref(), Some("a()"));
    }

    #[test]
    fn suggestions_are_capped_and_their_text_truncated() {
        let many: Vec<Value> = (0..300)
            .map(|n| json!({"label": format!("s{n}")}))
            .collect();
        let suggestions = suggestions_from(&Value::Array(many));
        assert_eq!(suggestions.len(), MAX_SUGGESTIONS);
        let long = "x".repeat(MAX_FIELD_CHARS + 100);
        let capped = suggestions_from(&json!([{"label": long}]));
        assert_eq!(capped[0].label.chars().count(), MAX_FIELD_CHARS);
    }

    #[test]
    fn completion_kinds_map_to_the_page_kinds() {
        assert_eq!(suggestion_kind(Some(3)), SuggestionKind::Function);
        assert_eq!(suggestion_kind(Some(6)), SuggestionKind::Variable);
        assert_eq!(suggestion_kind(Some(9)), SuggestionKind::Module);
        assert_eq!(suggestion_kind(Some(10)), SuggestionKind::Property);
        assert_eq!(suggestion_kind(Some(14)), SuggestionKind::Keyword);
        assert_eq!(suggestion_kind(Some(15)), SuggestionKind::Snippet);
        assert_eq!(suggestion_kind(Some(99)), SuggestionKind::Other);
        assert_eq!(suggestion_kind(None), SuggestionKind::Other);
    }

    #[test]
    fn a_hover_reads_markup_strings_and_lists() {
        let markup = json!({"contents": {"kind": "markdown", "value": " `echo` "}});
        assert_eq!(hover_text_from(&markup).as_deref(), Some("`echo`"));
        let plain = json!({"contents": "texte"});
        assert_eq!(hover_text_from(&plain).as_deref(), Some("texte"));
        let list = json!({"contents": ["a", {"language": "sh", "value": "b"}]});
        assert_eq!(hover_text_from(&list).as_deref(), Some("a\n\nb"));
    }

    #[test]
    fn an_empty_or_missing_hover_gives_nothing() {
        assert_eq!(hover_text_from(&Value::Null), None);
        assert_eq!(hover_text_from(&json!({"contents": "  "})), None);
        assert_eq!(hover_text_from(&json!({"contents": []})), None);
    }

    #[test]
    fn a_hover_is_truncated() {
        let long = "y".repeat(MAX_HOVER_CHARS + 10);
        let text = hover_text_from(&json!({"contents": long})).unwrap_or_default();
        assert_eq!(text.chars().count(), MAX_HOVER_CHARS);
    }

    fn sample_diagnostic(code: Value) -> Value {
        json!({
            "range": {
                "start": {"line": 1, "character": 2},
                "end": {"line": 1, "character": 6},
            },
            "severity": 2,
            "message": "problème",
            "source": "shellcheck",
            "code": code,
        })
    }

    #[test]
    fn a_diagnostic_keeps_its_range_severity_source_and_code() {
        let items = json!([sample_diagnostic(json!(2154))]);
        let diagnostics = diagnostics_from(&items, LanguageServer::Bash);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
        assert_eq!(diagnostics[0].range.start.character, 2);
        assert_eq!(diagnostics[0].range.end.character, 6);
        assert_eq!(diagnostics[0].source.as_deref(), Some("shellcheck"));
        assert_eq!(diagnostics[0].code.as_deref(), Some("2154"));
    }

    #[test]
    fn severities_follow_the_protocol_and_default_to_an_error() {
        assert_eq!(severity_from(Some(1)), DiagnosticSeverity::Error);
        assert_eq!(severity_from(Some(2)), DiagnosticSeverity::Warning);
        assert_eq!(severity_from(Some(3)), DiagnosticSeverity::Information);
        assert_eq!(severity_from(Some(4)), DiagnosticSeverity::Hint);
        assert_eq!(severity_from(None), DiagnosticSeverity::Error);
    }

    #[test]
    fn unresolved_imports_are_hidden_for_python_only() {
        let items = json!([
            sample_diagnostic(json!("unresolved-import")),
            sample_diagnostic(json!("unresolved-reference")),
        ]);
        let python = diagnostics_from(&items, LanguageServer::Python);
        assert_eq!(python.len(), 1);
        assert_eq!(python[0].code.as_deref(), Some("unresolved-reference"));
        let bash = diagnostics_from(&items, LanguageServer::Bash);
        assert_eq!(bash.len(), 2);
    }

    #[test]
    fn pyright_import_codes_are_hidden_too() {
        let items = json!([
            sample_diagnostic(json!("reportMissingImports")),
            sample_diagnostic(json!("reportMissingModuleSource")),
        ]);
        assert!(diagnostics_from(&items, LanguageServer::Python).is_empty());
    }

    #[test]
    fn a_malformed_diagnostic_is_skipped() {
        let items = json!([{"message": "sans étendue"}, sample_diagnostic(json!(1))]);
        assert_eq!(diagnostics_from(&items, LanguageServer::Bash).len(), 1);
        assert!(diagnostics_from(&Value::Null, LanguageServer::Bash).is_empty());
    }

    #[test]
    fn diagnostics_are_capped() {
        let items: Vec<Value> = (0..MAX_DIAGNOSTICS + 20)
            .map(|_| sample_diagnostic(json!(1)))
            .collect();
        let diagnostics = diagnostics_from(&Value::Array(items), LanguageServer::Bash);
        assert_eq!(diagnostics.len(), MAX_DIAGNOSTICS);
    }
}
