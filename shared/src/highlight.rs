//! Coloration syntaxique d'un code : jetons typés calculés par le central, rendus par le client.

use crate::exposed_variables::RequirementStatus;
use crate::local::LocalScriptSummary;
use serde::{Deserialize, Serialize};

/// Catégorie d'un fragment de code, traduite en couleur par le thème.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    #[default]
    Plain,
    Comment,
    String,
    Number,
    Keyword,
    Function,
    Variable,
    Operator,
}

impl TokenKind {
    /// Classe CSS du thème associée à la catégorie.
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Plain => "tok-plain",
            Self::Comment => "tok-comment",
            Self::String => "tok-string",
            Self::Number => "tok-number",
            Self::Keyword => "tok-keyword",
            Self::Function => "tok-function",
            Self::Variable => "tok-variable",
            Self::Operator => "tok-operator",
        }
    }
}

/// Fragment de texte d'une ligne de code, sans saut de ligne.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeToken {
    pub text: String,
    #[serde(default)]
    pub kind: TokenKind,
}

/// Aperçu d'un script de dépôt : ses métadonnées et son code découpé en lignes de jetons.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPreviewView {
    pub script: LocalScriptSummary,
    pub lines: Vec<Vec<CodeToken>>,
    /// Variables exposées par des plugins que le script consomme, et leur disponibilité ici.
    #[serde(default)]
    pub requirements: Vec<RequirementStatus>,
    /// Script intégré sans ses dépendances : elles ne sont pas contrôlées.
    #[serde(default)]
    pub dependencies_unverified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_kind_serializes_to_snake_case_and_has_unique_css_classes()
    -> Result<(), serde_json::Error> {
        assert_eq!(serde_json::to_string(&TokenKind::Keyword)?, "\"keyword\"");
        let kinds = [
            TokenKind::Plain,
            TokenKind::Comment,
            TokenKind::String,
            TokenKind::Number,
            TokenKind::Keyword,
            TokenKind::Function,
            TokenKind::Variable,
            TokenKind::Operator,
        ];
        let mut classes: Vec<&str> = kinds.iter().map(|kind| kind.css_class()).collect();
        classes.sort_unstable();
        classes.dedup();
        assert_eq!(classes.len(), kinds.len());
        let token: CodeToken = serde_json::from_str(r#"{"text":"ls"}"#)?;
        assert_eq!(token.kind, TokenKind::Plain);
        Ok(())
    }
}
