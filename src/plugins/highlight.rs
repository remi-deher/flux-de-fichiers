//! Coloration syntaxique des scripts avec les grammaires embarquées de syntect.

use forgearr_shared::highlight::{CodeToken, TokenKind};
use std::sync::LazyLock;
use syntect::{
    parsing::{ParseState, Scope, ScopeStack, SyntaxSet},
    util::LinesWithEndings,
};

/// Portées syntect reconnues, de la plus prioritaire à la moins prioritaire pour une même portée.
const SCOPE_RULES: &[(&str, TokenKind)] = &[
    ("comment", TokenKind::Comment),
    ("string", TokenKind::String),
    ("constant.numeric", TokenKind::Number),
    ("constant.language", TokenKind::Keyword),
    ("keyword.operator", TokenKind::Operator),
    ("keyword", TokenKind::Keyword),
    ("storage", TokenKind::Keyword),
    ("entity.name.function", TokenKind::Function),
    ("support.function", TokenKind::Function),
    ("variable.function", TokenKind::Function),
    ("variable", TokenKind::Variable),
];

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

static RULES: LazyLock<Vec<(Scope, TokenKind)>> = LazyLock::new(|| {
    SCOPE_RULES
        .iter()
        .filter_map(|(name, kind)| Scope::new(name).ok().map(|scope| (scope, *kind)))
        .collect()
});

/// Catégorie du fragment courant : la portée la plus interne reconnue l'emporte.
fn kind_of(stack: &ScopeStack) -> TokenKind {
    stack
        .as_slice()
        .iter()
        .rev()
        .find_map(|scope| {
            RULES
                .iter()
                .find(|(rule, _)| rule.is_prefix_of(*scope))
                .map(|(_, kind)| *kind)
        })
        .unwrap_or_default()
}

/// Ajoute un fragment (sans fin de ligne) en fusionnant avec le précédent de même catégorie.
fn push_token(tokens: &mut Vec<CodeToken>, text: &str, kind: TokenKind) {
    let text = text.trim_end_matches(['\n', '\r']);
    if text.is_empty() {
        return;
    }
    match tokens.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(text),
        _ => tokens.push(CodeToken {
            text: text.to_owned(),
            kind,
        }),
    }
}

fn plain_lines(code: &str) -> Vec<Vec<CodeToken>> {
    code.lines()
        .map(|line| {
            let mut tokens = Vec::new();
            push_token(&mut tokens, line, TokenKind::Plain);
            tokens
        })
        .collect()
}

fn highlight_with(
    syntaxes: &SyntaxSet,
    extension: &str,
    code: &str,
) -> Option<Vec<Vec<CodeToken>>> {
    let syntax = syntaxes.find_syntax_by_extension(extension)?;
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let operations = state.parse_line(line, syntaxes).ok()?;
        let mut tokens = Vec::new();
        let mut start = 0;
        for (position, operation) in operations {
            push_token(&mut tokens, line.get(start..position)?, kind_of(&stack));
            stack.apply(&operation).ok()?;
            start = position;
        }
        push_token(&mut tokens, line.get(start..)?, kind_of(&stack));
        lines.push(tokens);
    }
    Some(lines)
}

/// Découpe `code` en lignes de jetons selon l'extension du langage ; texte brut si le langage
/// n'a pas de grammaire embarquée ou si l'analyse échoue.
pub fn highlight_code(code: &str, extension: &str) -> Vec<Vec<CodeToken>> {
    highlight_with(&SYNTAXES, extension, code).unwrap_or_else(|| plain_lines(code))
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

    fn has_token(lines: &[Vec<CodeToken>], text: &str, kind: TokenKind) -> bool {
        lines
            .iter()
            .flatten()
            .any(|token| token.kind == kind && token.text.contains(text))
    }

    #[test]
    fn colors_shell_comments_and_strings() {
        let code = "# note\necho \"salut\"\n";
        let lines = highlight_code(code, "sh");
        assert!(has_token(&lines, "# note", TokenKind::Comment));
        assert!(has_token(&lines, "salut", TokenKind::String));
        assert_eq!(rebuild(&lines), code.trim_end());
    }

    #[test]
    fn colors_python_keywords_and_numbers() {
        let code = "def run():\n    return 42\n";
        let lines = highlight_code(code, "py");
        assert!(has_token(&lines, "def", TokenKind::Keyword));
        assert!(has_token(&lines, "42", TokenKind::Number));
        assert_eq!(rebuild(&lines), code.trim_end());
    }

    #[test]
    fn colors_javascript_keywords() {
        let lines = highlight_code("const value = 1;\n", "js");
        assert!(has_token(&lines, "const", TokenKind::Keyword));
    }

    #[test]
    fn unknown_extension_falls_back_to_plain_text_and_keeps_every_line() {
        let code = "première\n\nTroisième";
        let lines = highlight_code(code, "inconnu");
        assert_eq!(lines.len(), 3);
        assert!(lines[1].is_empty());
        assert!(
            lines
                .iter()
                .flatten()
                .all(|token| token.kind == TokenKind::Plain)
        );
        assert_eq!(rebuild(&lines), code);
    }

    #[test]
    fn powershell_never_loses_text_whether_or_not_a_grammar_is_embedded() {
        let code = "$x = 1\nWrite-Output $x\n";
        assert_eq!(rebuild(&highlight_code(code, "ps1")), code.trim_end());
    }

    #[test]
    fn empty_code_has_no_lines() {
        assert!(highlight_code("", "sh").is_empty());
    }
}
