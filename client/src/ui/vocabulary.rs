//! Vocabulaire unique de l'interface (ADR 0033) : les libellés partagés par la navigation, les titres,
//! les tiroirs et les états vides. Seul le texte affiché change ; routes d'API, champs JSON et noms de
//! modules gardent `flow`, `plugin` et `node` (ADR 0033, hypothèse H1).

/// Nom d'un flux, au singulier.
pub const FLOW: &str = "Flux";
/// Nom des flux : invariable en français.
pub const FLOWS: &str = FLOW;
/// Entrée de navigation et nom des dossiers de médias surveillés.
pub const LIBRARIES: &str = "Bibliothèques";
/// Nom d'une extension, au singulier.
pub const EXTENSION: &str = "Extension";
/// Entrée de navigation et titre de la liste des extensions.
pub const EXTENSIONS: &str = "Extensions";
/// Onglet où l'on parcourt et installe les éléments des dépôts (ancien « Store »).
pub const CATALOGUE: &str = "Catalogue";
/// Entrée de navigation des ressources, modèles de blocs et packs locaux (hors flux et bibliothèques).
pub const WORKSHOP: &str = "Atelier";
/// Libellé du filtre de fichiers d'un flux (ancien « masque » ; le champ d'API `fileMask` ne change pas).
pub const FILE_TYPES: &str = "Types de fichiers";

/// Mots qui ne doivent plus apparaître dans un texte affiché, en minuscules.
#[cfg(test)]
const BANNED_WORDS: &[&str] = &[
    "flow",
    "flows",
    "librairie",
    "librairies",
    "plugin",
    "plugins",
    "nœud",
    "nœuds",
    "noeud",
    "noeuds",
    "store",
    "masque",
    "masques",
    "pipeline",
    "pipelines",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const TEST_MODULE_MARKER: &str = "#[cfg(test)]";

    /// Littéraux de chaîne d'un source Rust, avec leur numéro de ligne ; commentaires ignorés.
    fn string_literals(source: &str) -> Vec<(usize, String)> {
        let chars: Vec<char> = source.chars().collect();
        let mut found = Vec::new();
        let mut line = 1;
        let mut index = 0;
        while index < chars.len() {
            let current = chars[index];
            let next = chars.get(index + 1).copied();
            if current == '\n' {
                line += 1;
                index += 1;
            } else if current == '/' && next == Some('/') {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
            } else if current == '/' && next == Some('*') {
                index += 2;
                while index < chars.len() && !is_block_comment_end(&chars, index) {
                    if chars[index] == '\n' {
                        line += 1;
                    }
                    index += 1;
                }
                index += 2;
            } else if current == '\'' {
                // Littéral de caractère (`'x'`, `'\n'`) ou durée de vie (`'a`).
                index += if next == Some('\\') {
                    chars[index + 2..]
                        .iter()
                        .position(|&candidate| candidate == '\'')
                        .map_or(chars.len() - index, |offset| offset + 3)
                } else if chars.get(index + 2) == Some(&'\'') {
                    3
                } else {
                    1
                };
            } else if current == '"' {
                let start_line = line;
                let mut text = String::new();
                index += 1;
                while index < chars.len() && chars[index] != '"' {
                    if chars[index] == '\\' {
                        index += 1;
                    }
                    if let Some(&character) = chars.get(index) {
                        text.push(character);
                        line += usize::from(character == '\n');
                    }
                    index += 1;
                }
                index += 1;
                found.push((start_line, text));
            } else {
                index += 1;
            }
        }
        found
    }

    fn is_block_comment_end(chars: &[char], index: usize) -> bool {
        chars[index] == '*' && chars.get(index + 1) == Some(&'/')
    }

    /// Mots d'un texte : lettres, tirets, soulignés et barres restent collés (`flows-view`, `/api/flows`).
    fn words(text: &str) -> impl Iterator<Item = &str> {
        text.split(|character: char| {
            !(character.is_alphabetic() || matches!(character, '-' | '_' | '/' | '#'))
        })
        .filter(|word| !word.is_empty())
    }

    /// Premier mot banni d'un littéral affiché. Un littéral sans espace ni majuscule initiale est un
    /// identifiant (route, classe, clé JSON) : il n'est pas du texte affiché.
    fn banned_word(literal: &str) -> Option<String> {
        let displayed = literal.chars().any(char::is_whitespace)
            || literal.chars().next().is_some_and(char::is_uppercase);
        if !displayed {
            return None;
        }
        words(literal)
            .map(str::to_lowercase)
            .find(|word| BANNED_WORDS.contains(&word.as_str()))
    }

    fn sources(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                sources(&path, found)?;
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
        Ok(())
    }

    #[test]
    fn the_shared_labels_follow_the_glossary() {
        assert_eq!(FLOWS, "Flux");
        assert_eq!(LIBRARIES, "Bibliothèques");
        assert_eq!(EXTENSIONS, "Extensions");
        assert_eq!(CATALOGUE, "Catalogue");
        assert_eq!(FILE_TYPES, "Types de fichiers");
        assert_eq!(WORKSHOP, "Atelier");
    }

    #[test]
    fn displayed_text_with_a_banned_word_is_flagged() {
        assert_eq!(banned_word("Flows & Modèles").as_deref(), Some("flows"));
        let libraries = banned_word("Filtrer les librairies…");
        assert_eq!(libraries.as_deref(), Some("librairies"));
        let plugin = banned_word("Ce plugin n'a pas de réglage.");
        assert_eq!(plugin.as_deref(), Some("plugin"));
        assert_eq!(banned_word("Nœud distant").as_deref(), Some("nœud"));
        assert_eq!(banned_word("Masque de fichiers").as_deref(), Some("masque"));
        assert_eq!(banned_word("Store").as_deref(), Some("store"));
    }

    #[test]
    fn identifiers_and_the_new_vocabulary_are_not_flagged() {
        assert_eq!(banned_word("/api/v1/flows"), None);
        assert_eq!(banned_word("plugin-icon-url"), None);
        assert_eq!(banned_word("stopPipeline"), None);
        assert_eq!(banned_word("tab-panel flows-view"), None);
        assert_eq!(banned_word("Masquer les composants"), None);
        assert_eq!(banned_word("Aucun flux. Une extension du Catalogue."), None);
    }

    #[test]
    fn the_scanner_reads_literals_and_skips_comments_and_characters() {
        let source = concat!(
            "// \"commentaire\"\n",
            "let a = \"un \\\"flow\\\"\"; let b = '\"'; /* \"bloc\" */\n",
            "let c = \"deux\";",
        );
        let literals = string_literals(source);
        assert_eq!(
            literals,
            vec![(2, "un \"flow\"".to_owned()), (3, "deux".to_owned())]
        );
    }

    #[test]
    fn no_banned_word_is_displayed_by_the_client() -> std::io::Result<()> {
        let mut files = Vec::new();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        sources(&root, &mut files)?;
        assert!(!files.is_empty());
        let mut offenders = Vec::new();
        for path in files {
            let content = std::fs::read_to_string(&path)?;
            // Les modules de tests, en fin de fichier, citent les anciens termes dans leurs jeux d'essai.
            let production = content.split(TEST_MODULE_MARKER).next().unwrap_or_default();
            for (line, literal) in string_literals(production) {
                if let Some(word) = banned_word(&literal) {
                    let place = path.display();
                    offenders.push(format!("{place}:{line} « {word} » dans {literal:?}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "termes bannis par l'ADR 0033 :\n{}",
            offenders.join("\n")
        );
        Ok(())
    }
}
