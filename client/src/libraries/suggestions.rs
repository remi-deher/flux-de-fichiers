//! Suggestions de code de l'éditeur de script (ADR 0032, phase a) : calculées côté client, sans serveur de
//! langage. Sources : champs du contrat JSON, variables déclarées, mots-clés de l'interpréteur et mots déjà
//! présents dans le script.

/// Nombre minimal de caractères tapés avant de proposer quoi que ce soit.
const MIN_PREFIX_CHARS: usize = 2;
/// Nombre maximal de suggestions affichées.
const MAX_SUGGESTIONS: usize = 8;
/// Longueur minimale d'un mot du script pour être proposé.
const MIN_WORD_CHARS: usize = 3;

/// Champs lus dans l'entrée JSON du script et écrits dans sa réponse (ADR 0017 et 0025).
const CONTRACT_FIELDS: &[&str] = &[
    "schemaVersion",
    "flowId",
    "nodeId",
    "operationId",
    "workingFile",
    "sourceFile",
    "workDir",
    "mediaInfo",
    "config",
    "variables",
    "status",
    "message",
    "nextPort",
    "stopPipeline",
];

const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "return", "exit", "echo", "export", "local", "read", "shift", "set", "unset",
    "trap", "eval", "exec", "test", "printf",
];
const PYTHON_KEYWORDS: &[&str] = &[
    "def", "class", "return", "import", "from", "as", "if", "elif", "else", "for", "while", "in",
    "try", "except", "finally", "with", "lambda", "yield", "pass", "break", "continue", "raise",
    "assert", "global", "nonlocal", "True", "False", "None", "print", "json", "sys",
];
const SCRIPT_KEYWORDS: &[&str] = &[
    "const",
    "let",
    "var",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "break",
    "continue",
    "try",
    "catch",
    "finally",
    "throw",
    "new",
    "class",
    "extends",
    "import",
    "export",
    "from",
    "async",
    "await",
    "typeof",
    "instanceof",
    "null",
    "undefined",
    "true",
    "false",
    "console",
    "JSON",
    "process",
];
const POWERSHELL_KEYWORDS: &[&str] = &[
    "function",
    "param",
    "if",
    "elseif",
    "else",
    "foreach",
    "for",
    "while",
    "switch",
    "return",
    "try",
    "catch",
    "finally",
    "throw",
    "Write-Output",
    "Write-Host",
    "Write-Error",
    "ConvertFrom-Json",
    "ConvertTo-Json",
    "Get-Content",
    "Set-Content",
    "Test-Path",
    "Join-Path",
];

/// Origine d'une suggestion ; l'ordre des variantes est l'ordre d'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SuggestionKind {
    Variable,
    Contract,
    Keyword,
    Word,
}

impl SuggestionKind {
    /// Étiquette montrée à droite de la suggestion.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Variable => "variable",
            Self::Contract => "contrat",
            Self::Keyword => "mot-clé",
            Self::Word => "mot",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub label: String,
    pub kind: SuggestionKind,
}

/// Mots-clés de la grammaire (`extension` : celle de l'interpréteur) ; aucun pour une extension inconnue.
fn keywords(extension: &str) -> &'static [&'static str] {
    match extension {
        "sh" => SHELL_KEYWORDS,
        "py" => PYTHON_KEYWORDS,
        "js" => SCRIPT_KEYWORDS,
        "ps1" => POWERSHELL_KEYWORDS,
        _ => &[],
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Début (en unités UTF-16, comme le navigateur) et texte du mot qui précède le curseur.
pub fn word_before(code: &str, caret: usize) -> (usize, String) {
    let units: Vec<u16> = code.encode_utf16().collect();
    let caret = caret.min(units.len());
    let before = String::from_utf16_lossy(&units[..caret]);
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| is_word_char(*c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (caret - word.encode_utf16().count(), word)
}

/// Ligne et colonne (en caractères, à partir de 0) du curseur, pour placer la liste.
pub fn caret_position(code: &str, caret: usize) -> (usize, usize) {
    let units: Vec<u16> = code.encode_utf16().collect();
    let before = String::from_utf16_lossy(&units[..caret.min(units.len())]);
    let line = before.matches('\n').count();
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count());
    (line, column)
}

/// Remplace `[start, end)` (UTF-16) par `text` ; renvoie le nouveau code et la position du curseur après.
pub fn replace_range(code: &str, start: usize, end: usize, text: &str) -> (String, usize) {
    let units: Vec<u16> = code.encode_utf16().collect();
    let end = end.min(units.len());
    let start = start.min(end);
    let inserted: Vec<u16> = text.encode_utf16().collect();
    let caret = start + inserted.len();
    let mut next = units[..start].to_vec();
    next.extend_from_slice(&inserted);
    next.extend_from_slice(&units[end..]);
    (String::from_utf16_lossy(&next), caret)
}

fn starts_with_ignore_case(label: &str, prefix: &str) -> bool {
    label.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// Suggestions pour `prefix`, de la source la plus précise à la plus générale, sans doublon.
pub fn suggest(prefix: &str, code: &str, extension: &str, declared: &[String]) -> Vec<Suggestion> {
    if prefix.chars().count() < MIN_PREFIX_CHARS {
        return Vec::new();
    }
    let script_words = code
        .split(|c: char| !is_word_char(c))
        .filter(|word| word.chars().count() >= MIN_WORD_CHARS);
    let sources = declared
        .iter()
        .map(|name| (name.as_str(), SuggestionKind::Variable))
        .chain(
            CONTRACT_FIELDS
                .iter()
                .map(|name| (*name, SuggestionKind::Contract)),
        )
        .chain(
            keywords(extension)
                .iter()
                .map(|name| (*name, SuggestionKind::Keyword)),
        )
        .chain(script_words.map(|word| (word, SuggestionKind::Word)));
    let mut found: Vec<Suggestion> = Vec::new();
    for (label, kind) in sources {
        if label != prefix
            && starts_with_ignore_case(label, prefix)
            && !found.iter().any(|known| known.label == label)
        {
            found.push(Suggestion {
                label: label.to_owned(),
                kind,
            });
        }
    }
    found.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.label.cmp(&b.label)));
    found.truncate(MAX_SUGGESTIONS);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[Suggestion]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn the_word_before_the_caret_is_found_with_its_start() {
        assert_eq!(word_before("echo wor", 8), (5, "wor".to_owned()));
        assert_eq!(word_before("echo wor", 4), (0, "echo".to_owned()));
        assert_eq!(word_before("a = ", 4), (4, String::new()));
        assert_eq!(word_before("x", 99), (0, "x".to_owned()));
    }

    #[test]
    fn positions_count_utf16_units_but_columns_count_characters() {
        // « 😀 » vaut 2 unités UTF-16 mais 1 caractère.
        assert_eq!(word_before("😀ab", 4), (2, "ab".to_owned()));
        assert_eq!(caret_position("a\n😀b", 5), (1, 2));
        assert_eq!(caret_position("abc", 0), (0, 0));
    }

    #[test]
    fn a_suggestion_replaces_the_typed_prefix() {
        assert_eq!(
            replace_range("echo wor", 5, 8, "workingFile"),
            ("echo workingFile".to_owned(), 16)
        );
        assert_eq!(replace_range("ab", 9, 9, "c"), ("abc".to_owned(), 3));
    }

    #[test]
    fn nothing_is_suggested_below_two_characters() {
        assert!(suggest("w", "", "sh", &[]).is_empty());
        assert!(suggest("", "", "sh", &[]).is_empty());
    }

    #[test]
    fn contract_fields_come_before_keywords_and_script_words() {
        let items = suggest("wor", "workspace=1", "sh", &[]);
        assert_eq!(labels(&items), ["workDir", "workingFile", "workspace"]);
        assert_eq!(items[0].kind, SuggestionKind::Contract);
        assert_eq!(items[2].kind, SuggestionKind::Word);
    }

    #[test]
    fn declared_variables_come_first_and_matching_ignores_case() {
        let declared = vec!["Quality".to_owned()];
        let items = suggest("qua", "", "py", &declared);
        assert_eq!(labels(&items), ["Quality"]);
        assert_eq!(items[0].kind, SuggestionKind::Variable);
    }

    #[test]
    fn keywords_depend_on_the_interpreter() {
        assert_eq!(labels(&suggest("fun", "", "sh", &[])), ["function"]);
        assert_eq!(labels(&suggest("lam", "", "py", &[])), ["lambda"]);
        assert_eq!(labels(&suggest("awa", "", "js", &[])), ["await"]);
        assert_eq!(labels(&suggest("Wri", "", "ps1", &[]))[0], "Write-Error");
        assert!(suggest("fun", "", "rb", &[]).is_empty());
    }

    #[test]
    fn the_typed_word_itself_and_duplicates_are_left_out() {
        let items = suggest("conf", "conf conf config config", "sh", &[]);
        assert_eq!(labels(&items), ["config"]);
    }

    #[test]
    fn at_most_eight_suggestions_are_returned() {
        let code = "aaa1 aaa2 aaa3 aaa4 aaa5 aaa6 aaa7 aaa8 aaa9 aaa10";
        assert_eq!(suggest("aa", code, "sh", &[]).len(), MAX_SUGGESTIONS);
    }
}
