//! Coloration PowerShell : les grammaires de syntect (moteur regex pur Rust) n'en embarquent pas,
//! et sa grammaire exige un moteur regex en C. Ce petit analyseur lexical couvre l'essentiel d'un
//! script : commentaires (ligne et bloc), chaînes (dont ici-chaînes), variables, nombres, mots-clés,
//! opérateurs `-eq` et cmdlets `Verbe-Nom`. Il ne perd jamais de texte.

use super::push_token;
use forgearr_shared::highlight::{CodeToken, TokenKind};

const KEYWORDS: &[&str] = &[
    "begin",
    "break",
    "catch",
    "class",
    "continue",
    "data",
    "do",
    "dynamicparam",
    "else",
    "elseif",
    "end",
    "exit",
    "filter",
    "finally",
    "for",
    "foreach",
    "from",
    "function",
    "if",
    "in",
    "parallel",
    "param",
    "process",
    "return",
    "switch",
    "throw",
    "trap",
    "try",
    "until",
    "using",
    "var",
    "while",
    "workflow",
];

const OPERATORS: &[&str] = &[
    "and",
    "as",
    "band",
    "bnot",
    "bor",
    "bxor",
    "ceq",
    "cge",
    "cgt",
    "cle",
    "clike",
    "clt",
    "cmatch",
    "cne",
    "contains",
    "eq",
    "f",
    "ge",
    "gt",
    "icontains",
    "ieq",
    "ilike",
    "imatch",
    "in",
    "ine",
    "is",
    "isnot",
    "join",
    "le",
    "like",
    "lt",
    "match",
    "ne",
    "not",
    "notcontains",
    "notin",
    "notlike",
    "notmatch",
    "or",
    "replace",
    "shl",
    "shr",
    "split",
    "xor",
];

const BLOCK_COMMENT_OPEN: &str = "<#";
const BLOCK_COMMENT_CLOSE: &str = "#>";

/// Où l'analyse reprend à la ligne suivante.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Code,
    BlockComment,
    /// Ici-chaîne ouverte par `@"` ou `@'` : se termine par ce guillemet suivi de `@`, en début de ligne.
    HereString(char),
}

/// Fragment reconnu : longueur en octets, catégorie, et nouvel état s'il change.
type Lexeme = (usize, TokenKind, Option<State>);

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Longueur d'une chaîne entre guillemets simples (`''` y échappe un guillemet) ou doubles
/// (`` ` `` ou `""` y échappent), guillemet ouvrant compris ; jusqu'à la fin si elle n'est pas fermée.
fn quoted_len(text: &str, quote: char) -> usize {
    let mut chars = text.char_indices().skip(1).peekable();
    while let Some((index, c)) = chars.next() {
        if c == '`' && quote == '"' {
            chars.next();
        } else if c == quote {
            if chars.peek().is_some_and(|(_, next)| *next == quote) {
                chars.next();
            } else {
                return index + c.len_utf8();
            }
        }
    }
    text.len()
}

fn variable_len(text: &str) -> usize {
    if let Some(inner) = text.strip_prefix("${") {
        return inner.find('}').map_or(text.len(), |end| end + 3);
    }
    let name: usize = text[1..]
        .chars()
        .take_while(|c| is_word_char(*c) || *c == ':')
        .map(char::len_utf8)
        .sum();
    if name > 0 {
        return 1 + name;
    }
    1 + text[1..].chars().next().map_or(0, |c| c.len_utf8())
}

fn word_kind(word: &str) -> TokenKind {
    let lower = word.to_lowercase();
    let is_cmdlet = word.chars().next().is_some_and(char::is_uppercase)
        && word
            .split_once('-')
            .is_some_and(|(_, noun)| noun.chars().next().is_some_and(char::is_uppercase));
    if KEYWORDS.contains(&lower.as_str()) {
        TokenKind::Keyword
    } else if is_cmdlet {
        TokenKind::Function
    } else {
        TokenKind::Plain
    }
}

fn lex_code(text: &str) -> Lexeme {
    let Some(first) = text.chars().next() else {
        return (0, TokenKind::Plain, None);
    };
    if let Some(after) = text.strip_prefix(BLOCK_COMMENT_OPEN) {
        return match after.find(BLOCK_COMMENT_CLOSE) {
            Some(end) => (
                BLOCK_COMMENT_OPEN.len() + end + BLOCK_COMMENT_CLOSE.len(),
                TokenKind::Comment,
                None,
            ),
            None => (text.len(), TokenKind::Comment, Some(State::BlockComment)),
        };
    }
    match first {
        '#' => (text.len(), TokenKind::Comment, None),
        '@' if matches!(text.get(1..2), Some("\"" | "'")) && text[2..].trim().is_empty() => {
            let quote = if text.as_bytes()[1] == b'"' {
                '"'
            } else {
                '\''
            };
            (
                text.len(),
                TokenKind::String,
                Some(State::HereString(quote)),
            )
        }
        '\'' | '"' => (quoted_len(text, first), TokenKind::String, None),
        '$' => (variable_len(text), TokenKind::Variable, None),
        c if c.is_ascii_digit() => {
            let len = text
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
                .unwrap_or(text.len());
            (len, TokenKind::Number, None)
        }
        '-' if text[1..].chars().next().is_some_and(char::is_alphabetic) => {
            let word_len = text[1..]
                .find(|c: char| !c.is_alphabetic())
                .unwrap_or(text.len() - 1);
            let kind = if OPERATORS.contains(&text[1..1 + word_len].to_lowercase().as_str()) {
                TokenKind::Operator
            } else {
                TokenKind::Plain
            };
            (1 + word_len, kind, None)
        }
        c if c.is_alphabetic() || c == '_' => {
            let len = text
                .find(|c: char| !(is_word_char(c) || c == '-'))
                .unwrap_or(text.len());
            (len, word_kind(&text[..len]), None)
        }
        c => (c.len_utf8(), TokenKind::Plain, None),
    }
}

/// Fragment suivant d'une ligne selon l'état ; `line_start` dit si `text` commence la ligne.
fn lex(text: &str, state: State, line_start: bool) -> Lexeme {
    match state {
        State::Code => lex_code(text),
        State::BlockComment => match text.find(BLOCK_COMMENT_CLOSE) {
            Some(end) => (
                end + BLOCK_COMMENT_CLOSE.len(),
                TokenKind::Comment,
                Some(State::Code),
            ),
            None => (text.len(), TokenKind::Comment, None),
        },
        State::HereString(quote) => {
            let closing = format!("{quote}@");
            if line_start && text.starts_with(&closing) {
                (closing.len(), TokenKind::String, Some(State::Code))
            } else {
                (text.len(), TokenKind::String, None)
            }
        }
    }
}

/// Découpe un script PowerShell en lignes de jetons.
pub fn highlight_powershell(code: &str) -> Vec<Vec<CodeToken>> {
    let mut state = State::Code;
    code.lines()
        .map(|line| {
            let mut tokens = Vec::new();
            let mut rest = line;
            while !rest.is_empty() {
                let (len, kind, next) = lex(rest, state, rest.len() == line.len());
                push_token(&mut tokens, &rest[..len], kind);
                rest = &rest[len..];
                if let Some(next) = next {
                    state = next;
                }
            }
            tokens
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rebuild(lines: &[Vec<CodeToken>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|token| token.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn has(lines: &[Vec<CodeToken>], text: &str, kind: TokenKind) -> bool {
        lines
            .iter()
            .flatten()
            .any(|token| token.kind == kind && token.text.contains(text))
    }

    #[test]
    fn colors_comments_strings_variables_numbers_and_cmdlets() {
        let code = "# note\n$cible = \"film $x\"\nif ($n -eq 42) { Write-Output 'ok' }\n";
        let lines = highlight_powershell(code);
        assert!(has(&lines, "# note", TokenKind::Comment));
        assert!(has(&lines, "$cible", TokenKind::Variable));
        assert!(has(&lines, "\"film $x\"", TokenKind::String));
        assert!(has(&lines, "if", TokenKind::Keyword));
        assert!(has(&lines, "-eq", TokenKind::Operator));
        assert!(has(&lines, "42", TokenKind::Number));
        assert!(has(&lines, "Write-Output", TokenKind::Function));
        assert!(has(&lines, "'ok'", TokenKind::String));
        assert_eq!(rebuild(&lines), code.trim_end());
    }

    #[test]
    fn block_comments_and_here_strings_span_lines() {
        let code = "<# début\nsuite #>\n$a = @\"\nligne $b\n\"@\nfin";
        let lines = highlight_powershell(code);
        assert!(has(&lines, "suite #>", TokenKind::Comment));
        assert!(has(&lines, "ligne $b", TokenKind::String));
        assert!(has(&lines, "\"@", TokenKind::String));
        assert!(!has(&lines, "fin", TokenKind::String));
        assert_eq!(rebuild(&lines), code);
    }

    #[test]
    fn escapes_do_not_end_a_string_early() {
        let lines = highlight_powershell("'l''a' \"a`\"b\" \"x\"\"y\"");
        let strings: Vec<&str> = lines[0]
            .iter()
            .filter(|token| token.kind == TokenKind::String)
            .map(|token| token.text.as_str())
            .collect();
        assert_eq!(strings, ["'l''a'", "\"a`\"b\"", "\"x\"\"y\""]);
    }

    #[test]
    fn unterminated_strings_variables_and_odd_input_never_lose_text() {
        for code in [
            "'ouverte",
            "\"ouverte `",
            "$",
            "${a b",
            "-",
            "@",
            "é$é -ünï 1.2.3",
            "<#",
            "x\ty",
        ] {
            assert_eq!(rebuild(&highlight_powershell(code)), code, "{code}");
        }
    }

    #[test]
    fn plain_words_and_parameters_stay_plain() {
        let lines = highlight_powershell("Get-ChildItem -Path mon-dossier");
        assert!(has(&lines, "Get-ChildItem", TokenKind::Function));
        assert!(!has(&lines, "-Path", TokenKind::Operator));
        assert!(!has(&lines, "mon-dossier", TokenKind::Function));
    }
}
