//! Info-bulles du serveur de langage dans l'éditeur (ADR 0032, lot c2) : la part pure. Le serveur répond en
//! Markdown ; il est réduit à des blocs de texte et de code, rendus comme du texte (jamais comme du HTML), de
//! sorte qu'une balise reçue du serveur n'a aucun effet.

/// Longueur maximale du texte affiché ; au-delà il est coupé.
const MAX_HOVER_CHARS: usize = 1500;
const CODE_FENCE: &str = "```";
const TRUNCATION_MARK: char = '…';

/// Bloc d'une info-bulle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoverBlock {
    Text(String),
    Code(String),
}

/// Retire les balises HTML (`<b>`, `</p>`, `<!-- … -->`) ; un `<` suivi d'autre chose (`a < b`) reste du texte.
fn without_tags(line: &str) -> String {
    let mut kept = String::new();
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        let opens_tag = character == '<'
            && characters
                .peek()
                .is_some_and(|next| next.is_alphabetic() || matches!(next, '/' | '!'));
        if opens_tag {
            for skipped in characters.by_ref() {
                if skipped == '>' {
                    break;
                }
            }
        } else {
            kept.push(character);
        }
    }
    kept
}

/// `[texte](adresse)` devient `texte` ; une parenthèse qui ne suit pas un crochet fermé reste telle quelle.
fn without_links(line: &str) -> String {
    let mut kept = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|index| open + index) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|index| close + index) else {
            break;
        };
        kept.push_str(&rest[..open]);
        kept.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    kept.push_str(rest);
    kept
}

/// Texte d'une ligne de Markdown sans ses marques (titres, gras, code en ligne, liens, balises).
fn plain_line(line: &str) -> String {
    let text = without_links(&without_tags(line));
    let text = text.trim_start_matches('#').trim_start();
    text.replace("**", "").replace("__", "").replace('`', "")
}

/// Une ligne qui n'est qu'un filet (`---`, `***`) ne dit rien.
fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|c| matches!(c, '-' | '*' | '_'))
}

fn truncated(text: &str) -> String {
    if text.chars().count() <= MAX_HOVER_CHARS {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(MAX_HOVER_CHARS).collect();
    cut.push(TRUNCATION_MARK);
    cut
}

fn flush_text(paragraph: &mut Vec<String>, blocks: &mut Vec<HoverBlock>) {
    if !paragraph.is_empty() {
        blocks.push(HoverBlock::Text(paragraph.join("\n")));
        paragraph.clear();
    }
}

/// Blocs de l'info-bulle : les paragraphes en texte, les blocs délimités par trois accents graves en code.
pub fn blocks(markdown: &str) -> Vec<HoverBlock> {
    let text = truncated(markdown);
    let mut blocks = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    for line in text.lines() {
        if line.trim_start().starts_with(CODE_FENCE) {
            match code.take() {
                Some(lines) => blocks.push(HoverBlock::Code(lines.join("\n"))),
                None => {
                    flush_text(&mut paragraph, &mut blocks);
                    code = Some(Vec::new());
                }
            }
        } else if let Some(lines) = code.as_mut() {
            lines.push(line);
        } else if line.trim().is_empty() || is_rule(line) {
            flush_text(&mut paragraph, &mut blocks);
        } else {
            paragraph.push(plain_line(line));
        }
    }
    flush_text(&mut paragraph, &mut blocks);
    if let Some(lines) = code {
        blocks.push(HoverBlock::Code(lines.join("\n")));
    }
    blocks.retain(|block| match block {
        HoverBlock::Text(text) | HoverBlock::Code(text) => !text.trim().is_empty(),
    });
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> HoverBlock {
        HoverBlock::Text(raw.to_owned())
    }

    fn code(raw: &str) -> HoverBlock {
        HoverBlock::Code(raw.to_owned())
    }

    #[test]
    fn paragraphs_and_fenced_code_become_blocks() {
        let markdown = "```sh\ndirname PATH\n```\n\nRetourne le dossier.\nSans le dernier élément.";
        assert_eq!(
            blocks(markdown),
            [
                code("dirname PATH"),
                text("Retourne le dossier.\nSans le dernier élément.")
            ]
        );
    }

    #[test]
    fn markdown_marks_are_removed() {
        let markdown =
            "# Titre\n**gras** et `code` et __souligné__\nVoir [la doc](https://exemple.org/a) ici";
        assert_eq!(
            blocks(markdown),
            [text("Titre\ngras et code et souligné\nVoir la doc ici")]
        );
    }

    #[test]
    fn html_from_the_server_never_survives() {
        let markdown = "<b>gras</b> <script>alert(1)</script> et <img src=x onerror=alert(1)>";
        let shown = blocks(markdown);
        assert_eq!(shown, [text("gras alert(1) et ")]);
    }

    #[test]
    fn a_lone_less_than_sign_is_text() {
        assert_eq!(blocks("si a < b alors"), [text("si a < b alors")]);
    }

    #[test]
    fn rules_and_empty_input_leave_nothing() {
        assert!(blocks("").is_empty());
        assert!(blocks("---\n\n***").is_empty());
        assert!(blocks("```\n```").is_empty());
    }

    #[test]
    fn an_unclosed_fence_keeps_its_code() {
        assert_eq!(blocks("```\nls -l"), [code("ls -l")]);
    }

    #[test]
    fn a_broken_link_stays_readable() {
        assert_eq!(blocks("[sans adresse"), [text("[sans adresse")]);
        assert_eq!(blocks("[a](b"), [text("[a](b")]);
    }

    #[test]
    fn the_text_is_cut_at_the_limit() {
        let long = "x".repeat(MAX_HOVER_CHARS * 2);
        let shown = blocks(&long);
        let [HoverBlock::Text(only)] = shown.as_slice() else {
            panic!("bloc inattendu : {shown:?}");
        };
        assert_eq!(only.chars().count(), MAX_HOVER_CHARS + 1);
        assert!(only.ends_with(TRUNCATION_MARK));
    }
}
