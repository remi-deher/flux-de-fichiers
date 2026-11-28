//! Couleurs reportées sur les lignes modifiées (ADR 0030) : tant que le central n'a pas recoloré le texte, une
//! ligne qu'on tape garde les couleurs de sa version précédente au lieu de repasser en blanc.
//!
//! Le début et la fin inchangés d'une ligne gardent la couleur de leurs caractères ; ce qui vient d'être
//! saisi continue une chaîne ou un commentaire ouvert, ou prend la couleur de ce qui l'entoure, et reste
//! sinon neutre jusqu'à la coloration du central.

use forgearr_shared::highlight::{CodeToken, TokenKind};

/// Un caractère et la couleur de son jeton.
type ColoredChar = (char, TokenKind);

fn flatten(tokens: &[CodeToken]) -> Vec<ColoredChar> {
    tokens
        .iter()
        .flat_map(|token| token.text.chars().map(|character| (character, token.kind)))
        .collect()
}

/// Regroupe les caractères voisins de même couleur en jetons.
fn regroup(chars: Vec<ColoredChar>) -> Vec<CodeToken> {
    let mut tokens: Vec<CodeToken> = Vec::new();
    for (character, kind) in chars {
        match tokens.last_mut() {
            Some(last) if last.kind == kind => last.text.push(character),
            _ => tokens.push(CodeToken {
                text: character.to_string(),
                kind,
            }),
        }
    }
    tokens
}

/// Couleur du texte saisi entre deux caractères inchangés.
fn inserted_kind(before: Option<TokenKind>, after: Option<TokenKind>) -> TokenKind {
    match (before, after) {
        (Some(before), Some(after)) if before == after => before,
        (Some(open @ (TokenKind::String | TokenKind::Comment)), _) => open,
        _ => TokenKind::Plain,
    }
}

/// La ligne `new_line`, colorée d'après les jetons `old` de sa version précédente.
pub fn carry(old: &[CodeToken], new_line: &str) -> Vec<CodeToken> {
    let old = flatten(old);
    let new: Vec<char> = new_line.chars().collect();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|((before, _), after)| before == *after)
        .count();
    let longest_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(longest_suffix)
        .take_while(|((before, _), after)| before == *after)
        .count();
    let kind_before = prefix
        .checked_sub(1)
        .and_then(|index| old.get(index))
        .map(|(_, kind)| *kind);
    let kind_after = (suffix > 0)
        .then(|| old.get(old.len() - suffix))
        .flatten()
        .map(|(_, kind)| *kind);
    let inserted = inserted_kind(kind_before, kind_after);
    let colored = new.iter().enumerate().map(|(index, character)| {
        let from_end = new.len() - index;
        let kind = if index < prefix {
            old.get(index).map(|(_, kind)| *kind)
        } else if from_end <= suffix {
            old.get(old.len() - from_end).map(|(_, kind)| *kind)
        } else {
            Some(inserted)
        };
        (*character, kind.unwrap_or_default())
    });
    regroup(colored.collect())
}

fn plain(text: &str) -> Vec<CodeToken> {
    vec![CodeToken {
        text: text.to_owned(),
        kind: TokenKind::Plain,
    }]
}

/// Lignes `new` colorées d'après les lignes `old` qu'elles remplacent : la ligne `i` reprend la ligne `i`,
/// celles en plus (saut de ligne, collage) reprennent la dernière.
pub fn carry_lines(old: &[Vec<CodeToken>], new: &[&str]) -> Vec<Vec<CodeToken>> {
    new.iter()
        .enumerate()
        .map(|(index, line)| match old.get(index).or(old.last()) {
            Some(tokens) => carry(tokens, line),
            None => plain(line),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str, kind: TokenKind) -> CodeToken {
        CodeToken {
            text: text.to_owned(),
            kind,
        }
    }

    #[test]
    fn an_unchanged_line_keeps_its_tokens() {
        let old = [
            token("echo", TokenKind::Keyword),
            token(" ", TokenKind::Plain),
            token("1", TokenKind::Number),
        ];
        assert_eq!(carry(&old, "echo 1"), old);
    }

    #[test]
    fn text_typed_after_a_word_stays_neutral() {
        let old = [token("echo", TokenKind::Keyword)];
        let expected = [
            token("echo", TokenKind::Keyword),
            token(" x", TokenKind::Plain),
        ];
        assert_eq!(carry(&old, "echo x"), expected);
    }

    #[test]
    fn typing_inside_a_word_keeps_its_color() {
        let old = [token("echo", TokenKind::Keyword)];
        assert_eq!(carry(&old, "eccho"), [token("eccho", TokenKind::Keyword)]);
    }

    #[test]
    fn typing_in_an_open_string_or_comment_continues_it() {
        let string = [
            token("x = ", TokenKind::Plain),
            token("\"ab", TokenKind::String),
        ];
        let expected = [
            token("x = ", TokenKind::Plain),
            token("\"abc", TokenKind::String),
        ];
        assert_eq!(carry(&string, "x = \"abc"), expected);
        let comment = [token("# note", TokenKind::Comment)];
        let expected = [token("# notes", TokenKind::Comment)];
        assert_eq!(carry(&comment, "# notes"), expected);
    }

    #[test]
    fn the_end_of_a_line_keeps_its_color_when_the_start_changes() {
        let old = [
            token("ls", TokenKind::Keyword),
            token(" # fin", TokenKind::Comment),
        ];
        let expected = [
            token("x", TokenKind::Plain),
            token(" # fin", TokenKind::Comment),
        ];
        assert_eq!(carry(&old, "x # fin"), expected);
    }

    #[test]
    fn deleted_text_leaves_the_rest_colored() {
        let old = [
            token("echo", TokenKind::Keyword),
            token(" x", TokenKind::Plain),
        ];
        let expected = [
            token("ech", TokenKind::Keyword),
            token(" x", TokenKind::Plain),
        ];
        assert_eq!(carry(&old, "ech x"), expected);
        assert!(carry(&old, "").is_empty());
    }

    #[test]
    fn a_line_with_no_previous_tokens_is_neutral() {
        assert_eq!(carry(&[], "abc"), [token("abc", TokenKind::Plain)]);
    }

    #[test]
    fn non_ascii_characters_are_kept_whole() {
        let old = [token("é", TokenKind::String)];
        assert_eq!(carry(&old, "é😀"), [token("é😀", TokenKind::String)]);
    }

    #[test]
    fn extra_lines_take_the_color_of_the_last_old_one() {
        let old = vec![
            vec![token("a", TokenKind::Keyword)],
            vec![token("# c", TokenKind::Comment)],
        ];
        let lines = carry_lines(&old, &["a", "# c", "# c"]);
        assert_eq!(lines[2], [token("# c", TokenKind::Comment)]);
        assert_eq!(lines.len(), 3);
        let none = carry_lines(&[], &["x"]);
        assert_eq!(none, [vec![token("x", TokenKind::Plain)]]);
    }
}
