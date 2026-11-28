//! Règles de diagnostic propres à Forgearr (ADR 0032, lot c3).
//!
//! Elles lisent le texte du script et ses variables déclarées, sans serveur de langage : elles servent aussi
//! quand aucun serveur n'est installé. Chaque règle ne signale que ce qu'elle reconnaît sans doute, jamais
//! une interprétation du code.

use forgearr_shared::flows::{BranchPort, SCRIPT_OUTPUT_FIELDS};
use forgearr_shared::language::{Diagnostic, DiagnosticSeverity, Position, Range};
use forgearr_shared::local::ScriptInterpreter;
use forgearr_shared::script_variables::RESERVED_VARIABLE_NAMES;
use std::ops::Range as ByteSpan;

/// Origine affichée de ces anomalies.
const SOURCE: &str = "Forgearr";
/// Nom, dans la charge reçue sur `stdin`, de l'objet qui porte les valeurs des variables.
const CONFIG_WORD: &str = "config";
/// Nom de la sortie JSON qui choisit le port suivi.
const NEXT_PORT_WORD: &str = "nextPort";
/// Méthode qui lit une clé (`config.get("nom")` en Python).
const GET_METHOD: &str = "get";
/// Fin du message d'une variable lue sans être déclarée (le nom la précède).
const UNDECLARED_TEXT: &str = "est lue mais n'est pas déclarée dans l'onglet Variables.";
/// Fin du message d'un port inconnu (le port la précède).
const UNKNOWN_PORT_TEXT: &str = "n'existe qu'en routage simple ; en routage conditionnel, seuls";
/// Message d'une sortie sans JSON.
const NO_JSON_TEXT: &str = "Le script n'écrit aucune réponse JSON sur stdout : le moteur le tient pour réussi, sans message, port ni variable.";
/// Nombre maximal d'anomalies rendues par ces règles.
const MAX_RULE_DIAGNOSTICS: usize = 100;

/// Règle qui a signalé l'anomalie : sert de code, que la page peut afficher ou filtrer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    UndeclaredVariable,
    UnknownPort,
    NoJsonOutput,
}

impl Rule {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UndeclaredVariable => "undeclared-variable",
            Self::UnknownPort => "unknown-port",
            Self::NoJsonOutput => "no-json-output",
        }
    }
}

/// Extraits qui montrent que le script écrit du JSON, selon son langage.
const fn json_markers(interpreter: ScriptInterpreter) -> &'static [&'static str] {
    match interpreter {
        ScriptInterpreter::Python3 => &["json.dumps", "json.dump("],
        ScriptInterpreter::Node | ScriptInterpreter::Deno | ScriptInterpreter::Bun => {
            &["JSON.stringify"]
        }
        ScriptInterpreter::Pwsh => &["ConvertTo-Json"],
        ScriptInterpreter::Sh | ScriptInterpreter::Bash => &["jq "],
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_quote_byte(byte: u8) -> bool {
    matches!(byte, b'"' | b'\'' | b'\\')
}

fn byte_at(code: &str, index: usize) -> Option<u8> {
    code.as_bytes().get(index).copied()
}

/// Premier octet à partir de `from` que `skip` ne saute pas.
fn skip_while(code: &str, from: usize, skip: impl Fn(u8) -> bool) -> usize {
    let mut index = from;
    while byte_at(code, index).is_some_and(&skip) {
        index += 1;
    }
    index
}

fn identifier_end(code: &str, from: usize) -> usize {
    skip_while(code, from, is_identifier_byte)
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// Position `(ligne, colonne UTF-16)` d'un octet du texte.
fn position_of(code: &str, byte: usize) -> Position {
    let before = code.get(..byte).unwrap_or_default();
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = before
        .get(line_start..)
        .unwrap_or_default()
        .encode_utf16()
        .count();
    Position {
        line: u32::try_from(before.matches('\n').count()).unwrap_or(u32::MAX),
        character: u32::try_from(column).unwrap_or(u32::MAX),
    }
}

fn diagnostic(
    code: &str,
    span: ByteSpan<usize>,
    severity: DiagnosticSeverity,
    rule: Rule,
    message: String,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: position_of(code, span.start),
            end: position_of(code, span.end),
        },
        severity,
        message,
        source: Some(SOURCE.to_owned()),
        code: Some(rule.code().to_owned()),
    }
}

/// Nom de variable lu juste après le mot `config` qui finit en `after_word`, et sa place dans le texte.
///
/// Reconnaît `config.nom`, `config["nom"]`, `config.get("nom")` et `["config"]["nom"]`.
fn config_name(code: &str, after_word: usize) -> Option<ByteSpan<usize>> {
    let mut cursor = skip_while(code, after_word, is_quote_byte);
    if byte_at(code, cursor) == Some(b']') {
        cursor += 1;
    }
    if byte_at(code, cursor) == Some(b'?') {
        cursor += 1;
    }
    match byte_at(code, cursor)? {
        b'.' => dotted_name(code, cursor + 1),
        b'[' => quoted_name(code, cursor + 1),
        _ => None,
    }
}

/// `config.nom`, ou `config.get("nom")` ; un autre appel de méthode ne lit aucune variable.
fn dotted_name(code: &str, from: usize) -> Option<ByteSpan<usize>> {
    let end = identifier_end(code, from);
    let name = code.get(from..end)?;
    if name.is_empty() {
        return None;
    }
    match byte_at(code, end) {
        Some(b'(') if name == GET_METHOD => quoted_name(code, end + 1),
        Some(b'(') => None,
        _ => Some(from..end),
    }
}

/// Nom entre guillemets : `"nom"` après un `[` ou un `(` ; sans guillemet, c'est une variable, pas un nom.
fn quoted_name(code: &str, from: usize) -> Option<ByteSpan<usize>> {
    let start = skip_while(code, from, is_blank);
    let name_start = skip_while(code, start, is_quote_byte);
    if name_start == start {
        return None;
    }
    let end = identifier_end(code, name_start);
    (end > name_start).then_some(name_start..end)
}

/// Noms lus dans `config` : chaque lecture, avec sa place dans le texte.
fn config_reads(code: &str) -> Vec<ByteSpan<usize>> {
    code.match_indices(CONFIG_WORD)
        .filter(|(start, _)| {
            let before = start.checked_sub(1).and_then(|index| byte_at(code, index));
            let after = byte_at(code, start + CONFIG_WORD.len());
            !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
        })
        .filter_map(|(start, word)| config_name(code, start + word.len()))
        .collect()
}

fn undeclared_variables(code: &str, declared: &[String]) -> Vec<Diagnostic> {
    config_reads(code)
        .into_iter()
        .filter_map(|span| {
            let name = code.get(span.clone())?;
            let known = declared.iter().any(|declared| declared == name)
                || RESERVED_VARIABLE_NAMES.contains(&name);
            (!known).then(|| {
                let message = format!("La variable « {name} » {UNDECLARED_TEXT}");
                let severity = DiagnosticSeverity::Information;
                diagnostic(code, span, severity, Rule::UndeclaredVariable, message)
            })
        })
        .collect()
}

/// Valeur littérale donnée à `nextPort` : `nextPort: "x"`, `"nextPort": "x"`, `nextPort = 'x'`, `\"nextPort\":\"x\"`.
fn next_port_values(code: &str) -> Vec<ByteSpan<usize>> {
    code.match_indices(NEXT_PORT_WORD)
        .filter_map(|(start, word)| {
            let after_key = skip_while(code, start + word.len(), is_quote_byte);
            let separator = skip_while(code, after_key, is_blank);
            if !matches!(byte_at(code, separator)?, b':' | b'=') {
                return None;
            }
            let value_open = skip_while(code, separator + 1, is_blank);
            let value_start = skip_while(code, value_open, is_quote_byte);
            if value_start == value_open {
                return None;
            }
            let value_end = skip_while(code, value_start, |byte| {
                !is_quote_byte(byte) && byte != b'\n'
            });
            (value_end > value_start).then_some(value_start..value_end)
        })
        .collect()
}

fn unknown_ports(code: &str) -> Vec<Diagnostic> {
    next_port_values(code)
        .into_iter()
        .filter_map(|span| {
            let value = code.get(span.clone())?;
            if BranchPort::parse(value).is_some() {
                return None;
            }
            let (accepted, other) = (BranchPort::True, BranchPort::False);
            let message = format!(
                "Le port « {value} » {UNKNOWN_PORT_TEXT} « {} » et « {} » sont acceptés.",
                accepted.as_str(),
                other.as_str()
            );
            let severity = DiagnosticSeverity::Information;
            Some(diagnostic(code, span, severity, Rule::UnknownPort, message))
        })
        .collect()
}

/// Le texte écrit un nom de champ de la réponse entre guillemets.
fn names_an_output_field(code: &str) -> bool {
    SCRIPT_OUTPUT_FIELDS.iter().any(|(name, _)| {
        code.contains(&format!("\"{name}\""))
            || code.contains(&format!("'{name}'"))
            || code.contains(&format!("\\\"{name}"))
    })
}

fn writes_json(interpreter: ScriptInterpreter, code: &str) -> bool {
    json_markers(interpreter)
        .iter()
        .any(|marker| code.contains(marker))
        || names_an_output_field(code)
}

/// Première ligne qui porte du texte : l'endroit où la règle de sortie s'affiche.
fn first_text_line(code: &str) -> Option<ByteSpan<usize>> {
    let mut start = 0;
    for line in code.split('\n') {
        let end = start + line.len();
        if !line.trim().is_empty() {
            return Some(start..start + line.trim_end().len());
        }
        start = end + 1;
    }
    None
}

fn missing_json_output(interpreter: ScriptInterpreter, code: &str) -> Option<Diagnostic> {
    if writes_json(interpreter, code) {
        return None;
    }
    let span = first_text_line(code)?;
    let message = NO_JSON_TEXT.to_owned();
    let severity = DiagnosticSeverity::Information;
    Some(diagnostic(
        code,
        span,
        severity,
        Rule::NoJsonOutput,
        message,
    ))
}

/// Anomalies relevées par les règles de Forgearr, dans l'ordre du texte.
pub fn check(interpreter: ScriptInterpreter, code: &str, declared: &[String]) -> Vec<Diagnostic> {
    let mut found = undeclared_variables(code, declared);
    found.extend(unknown_ports(code));
    found.extend(missing_json_output(interpreter, code));
    found.sort_by_key(|item| (item.range.start.line, item.range.start.character));
    found.truncate(MAX_RULE_DIAGNOSTICS);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYTHON: ScriptInterpreter = ScriptInterpreter::Python3;

    fn declared(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn codes(found: &[Diagnostic]) -> Vec<&str> {
        found
            .iter()
            .filter_map(|item| item.code.as_deref())
            .collect()
    }

    #[test]
    fn rule_codes_are_distinct_and_readable() {
        let all = [
            Rule::UndeclaredVariable,
            Rule::UnknownPort,
            Rule::NoJsonOutput,
        ];
        for (index, rule) in all.iter().enumerate() {
            assert!(!rule.code().is_empty());
            for other in all.iter().skip(index + 1) {
                assert_ne!(other.code(), rule.code());
            }
        }
    }

    #[test]
    fn a_declared_variable_read_in_every_style_is_accepted() {
        let known = declared(&["quality", "mode"]);
        let python = "config = payload[\"config\"]\nx = config[\"quality\"] + config.get('mode')";
        assert!(undeclared_variables(python, &known).is_empty());
        let javascript = "const q = input.config.quality + config['mode'];";
        assert!(undeclared_variables(javascript, &known).is_empty());
        let powershell = "$payload.config.quality";
        assert!(undeclared_variables(powershell, &known).is_empty());
        let shell = "jq -r '.config.quality' < in.json";
        assert!(undeclared_variables(shell, &known).is_empty());
    }

    #[test]
    fn an_undeclared_variable_is_marked_where_it_is_read() {
        let code = "import json\nvalue = config[\"speed\"]\n";
        let found = undeclared_variables(code, &declared(&["quality"]));
        let [item] = found.as_slice() else {
            panic!("une seule anomalie attendue, reçu {found:?}");
        };
        let start = Position {
            line: 1,
            character: 16,
        };
        let end = Position {
            line: 1,
            character: 21,
        };
        assert_eq!(item.range, Range { start, end });
        assert_eq!(item.severity, DiagnosticSeverity::Information);
        assert_eq!(item.code.as_deref(), Some("undeclared-variable"));
        assert_eq!(item.source.as_deref(), Some("Forgearr"));
        assert!(item.message.contains("« speed »"));
    }

    #[test]
    fn every_reading_of_an_undeclared_variable_is_marked() {
        let code = "a = config.speed\nb = config.get(\"speed\")\nc = p[\"config\"][\"speed\"]";
        assert_eq!(undeclared_variables(code, &declared(&[])).len(), 3);
    }

    #[test]
    fn other_uses_of_the_word_config_are_not_variable_reads() {
        let known = declared(&[]);
        let code = "myconfig.x = 1\nconfig_path = 2\nconfigs.y\nconfig.items()\nconfig[i]";
        assert!(undeclared_variables(code, &known).is_empty());
        assert!(undeclared_variables("config.get(key)", &known).is_empty());
        assert!(undeclared_variables("config", &known).is_empty());
        assert!(undeclared_variables("config = {}", &known).is_empty());
    }

    #[test]
    fn reserved_names_are_not_variables() {
        let code = "config.secrets";
        assert!(undeclared_variables(code, &declared(&[])).is_empty());
    }

    #[test]
    fn columns_count_utf16_units() {
        let code = "é = \"😀\"; v = config.vitesse";
        let found = undeclared_variables(code, &declared(&[]));
        let characters: Vec<u32> = found
            .iter()
            .map(|item| item.range.start.character)
            .collect();
        assert_eq!(characters, [21]);
    }

    #[test]
    fn a_port_other_than_true_or_false_is_flagged() {
        let code = "print(json.dumps({\"nextPort\": \"alt\"}))";
        let found = unknown_ports(code);
        let [item] = found.as_slice() else {
            panic!("une seule anomalie attendue, reçu {found:?}");
        };
        assert_eq!(item.code.as_deref(), Some("unknown-port"));
        assert!(item.message.contains("« alt »"));
        assert!(item.message.contains("« true » et « false » sont acceptés"));
        assert_eq!(item.range.start.character, 31);
        assert_eq!(item.range.end.character, 34);
    }

    #[test]
    fn true_and_false_ports_are_accepted_in_every_style() {
        for code in [
            "{\"nextPort\": \"true\"}",
            "{ nextPort: 'false' }",
            "@{ nextPort = \"true\" }",
            "printf '{\\\"nextPort\\\":\\\"false\\\"}'",
        ] {
            assert!(unknown_ports(code).is_empty(), "{code}");
        }
    }

    #[test]
    fn a_port_given_by_a_variable_is_not_flagged() {
        assert!(unknown_ports("{\"nextPort\": port}").is_empty());
        assert!(unknown_ports("payload.nextPort").is_empty());
        assert!(unknown_ports("nextPort").is_empty());
        assert!(unknown_ports("nextPort: \"\"").is_empty());
    }

    #[test]
    fn a_shell_port_with_escaped_quotes_is_read() {
        let code = "printf '{\\\"nextPort\\\":\\\"alt\\\"}'";
        let found = unknown_ports(code);
        let messages: Vec<&str> = found.iter().map(|item| item.message.as_str()).collect();
        assert_eq!(messages.len(), 1);
        assert!(messages.iter().all(|message| message.contains("« alt »")));
    }

    #[test]
    fn a_script_without_json_output_is_flagged_on_its_first_line() {
        let code = "\n  \nprint('bonjour')\nprint('suite')\n";
        let found = missing_json_output(PYTHON, code);
        let item = found.as_ref();
        assert_eq!(item.map(|found| found.range.start.line), Some(2));
        assert_eq!(item.map(|found| found.range.end.character), Some(16));
        let rule = item.and_then(|found| found.code.as_deref());
        assert_eq!(rule, Some("no-json-output"));
    }

    #[test]
    fn json_output_is_recognised_in_every_language() {
        let writers = [
            (PYTHON, "print(json.dumps(result))"),
            (ScriptInterpreter::Node, "console.log(JSON.stringify(r))"),
            (ScriptInterpreter::Deno, "console.log(JSON.stringify(r))"),
            (ScriptInterpreter::Bun, "console.log(JSON.stringify(r))"),
            (ScriptInterpreter::Pwsh, "$r | ConvertTo-Json"),
            (ScriptInterpreter::Bash, "jq -n '{status: 1}'"),
            (ScriptInterpreter::Sh, "printf '{\"status\":\"ok\"}'"),
            (PYTHON, "print('{\"message\": \"ok\"}')"),
        ];
        for (interpreter, code) in writers {
            assert!(missing_json_output(interpreter, code).is_none(), "{code}");
        }
    }

    #[test]
    fn an_empty_script_is_not_flagged() {
        assert!(missing_json_output(PYTHON, "").is_none());
        assert!(missing_json_output(PYTHON, "  \n\n").is_none());
    }

    #[test]
    fn the_starter_scripts_raise_nothing() {
        use super::super::script_draft::starter_script_code;
        for &interpreter in ScriptInterpreter::ALL {
            let code = starter_script_code(interpreter);
            assert!(check(interpreter, code, &[]).is_empty(), "{interpreter:?}");
        }
    }

    #[test]
    fn check_gathers_the_rules_in_text_order() {
        let code = "x = config.speed\ny = {\"nextPort\": \"alt\"}\n";
        let found = check(PYTHON, code, &[]);
        assert_eq!(codes(&found), ["undeclared-variable", "unknown-port"]);
        let silent = "print('a')\nx = config.speed\n";
        let found = check(PYTHON, silent, &[]);
        assert_eq!(codes(&found), ["no-json-output", "undeclared-variable"]);
    }

    #[test]
    fn check_is_capped() {
        let code = "config.speed\n".repeat(MAX_RULE_DIAGNOSTICS + 20);
        assert_eq!(check(PYTHON, &code, &[]).len(), MAX_RULE_DIAGNOSTICS);
    }
}
