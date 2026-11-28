//! Diagnostics du serveur de langage dans l'éditeur (ADR 0032, lot c1) : la part pure. Les anomalies sont
//! posées sur les jetons colorés (la couche `pre` garde exactement le texte saisi), dans la gouttière, et
//! résumées en une ligne. Les positions sont celles du protocole : ligne depuis 0, colonne en unités UTF-16.

use forgearr_shared::{
    highlight::{CodeToken, TokenKind},
    language::{Diagnostic, DiagnosticSeverity},
};

/// Fin d'une plage qui court jusqu'au bout de la ligne.
const END_OF_LINE: u32 = u32::MAX;

/// Fragment de ligne, avec la gravité de l'anomalie qui le recouvre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkedToken {
    pub text: String,
    pub kind: TokenKind,
    pub mark: Option<DiagnosticSeverity>,
}

/// Anomalie la plus grave d'une ligne, avec son message pour l'infobulle de la gouttière.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GutterMark {
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// 0 pour la plus grave.
const fn rank(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::Error => 0,
        DiagnosticSeverity::Warning => 1,
        DiagnosticSeverity::Information => 2,
        DiagnosticSeverity::Hint => 3,
    }
}

fn worse(current: Option<DiagnosticSeverity>, other: DiagnosticSeverity) -> DiagnosticSeverity {
    match current {
        Some(known) if rank(known) <= rank(other) => known,
        _ => other,
    }
}

/// Classe de style de la marque d'une gravité.
pub const fn mark_class(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "diag-error",
        DiagnosticSeverity::Warning => "diag-warning",
        DiagnosticSeverity::Information => "diag-information",
        DiagnosticSeverity::Hint => "diag-hint",
    }
}

/// Zone de la ligne `line` couverte par `diagnostic` : `[début, fin)` en unités UTF-16.
fn span_on_line(diagnostic: &Diagnostic, line: u32) -> Option<(u32, u32)> {
    let (start, end) = (diagnostic.range.start, diagnostic.range.end);
    if line < start.line || line > end.line {
        return None;
    }
    let from = if line == start.line {
        start.character
    } else {
        0
    };
    let to = if line == end.line {
        end.character
    } else {
        END_OF_LINE
    };
    // Une plage vide marque tout de même le caractère qu'elle touche.
    Some((from, to.max(from.saturating_add(1))))
}

/// Coupe `text` (qui commence à la colonne `base`) aux colonnes de `cuts`.
fn cut_at(text: &str, base: u32, cuts: &[u32]) -> Vec<(String, u32)> {
    let mut pieces: Vec<(String, u32)> = Vec::new();
    let mut offset = base;
    for character in text.chars() {
        let starts_piece = cuts.contains(&offset);
        match pieces.last_mut() {
            Some((piece, _)) if !starts_piece => piece.push(character),
            _ => pieces.push((character.to_string(), offset)),
        }
        offset += u32::try_from(character.len_utf16()).unwrap_or(1);
    }
    pieces
}

fn mark_line(line: &[CodeToken], spans: &[(u32, u32, DiagnosticSeverity)]) -> Vec<MarkedToken> {
    let cuts: Vec<u32> = spans
        .iter()
        .flat_map(|(from, to, _)| [*from, *to])
        .collect();
    let mut offset = 0_u32;
    let mut marked = Vec::new();
    for token in line {
        for (text, start) in cut_at(&token.text, offset, &cuts) {
            let mark = spans
                .iter()
                .filter(|(from, to, _)| *from <= start && start < *to)
                .fold(None, |known, (_, _, severity)| {
                    Some(worse(known, *severity))
                });
            marked.push(MarkedToken {
                text,
                kind: token.kind,
                mark,
            });
        }
        offset += u32::try_from(token.text.encode_utf16().count()).unwrap_or(u32::MAX);
    }
    marked
}

/// Jetons de chaque ligne, coupés aux bornes des anomalies et marqués de leur gravité. Le texte n'est jamais
/// modifié : la couche colorée garde la largeur de la saisie.
pub fn mark_lines(lines: &[Vec<CodeToken>], diagnostics: &[Diagnostic]) -> Vec<Vec<MarkedToken>> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let number = u32::try_from(index).unwrap_or(u32::MAX);
            let spans: Vec<(u32, u32, DiagnosticSeverity)> = diagnostics
                .iter()
                .filter_map(|diagnostic| {
                    span_on_line(diagnostic, number).map(|(from, to)| (from, to, diagnostic.severity))
                })
                .collect();
            mark_line(line, &spans)
        })
        .collect()
}

/// Marque de la gouttière de chacune des `line_count` lignes : l'anomalie la plus grave qui la touche.
pub fn gutter_marks(line_count: usize, diagnostics: &[Diagnostic]) -> Vec<Option<GutterMark>> {
    let mut marks: Vec<Option<GutterMark>> = vec![None; line_count];
    for diagnostic in diagnostics {
        let first = usize::try_from(diagnostic.range.start.line).unwrap_or(usize::MAX);
        let last = usize::try_from(diagnostic.range.end.line).unwrap_or(usize::MAX);
        for slot in marks.iter_mut().take(last.saturating_add(1)).skip(first) {
            let keeps = slot
                .as_ref()
                .is_some_and(|known| rank(known.severity) <= rank(diagnostic.severity));
            if !keeps {
                *slot = Some(GutterMark {
                    severity: diagnostic.severity,
                    message: diagnostic.message.clone(),
                });
            }
        }
    }
    marks
}

fn counted(count: usize, singular: &str, plural: &str) -> Option<String> {
    match count {
        0 => None,
        1 => Some(format!("1 {singular}")),
        many => Some(format!("{many} {plural}")),
    }
}

/// Résumé d'une ligne (« 2 erreurs · 1 avertissement ») ; `None` sans anomalie.
pub fn summary(diagnostics: &[Diagnostic]) -> Option<String> {
    let count = |wanted: &[DiagnosticSeverity]| {
        diagnostics
            .iter()
            .filter(|diagnostic| wanted.contains(&diagnostic.severity))
            .count()
    };
    let parts: Vec<String> = [
        counted(count(&[DiagnosticSeverity::Error]), "erreur", "erreurs"),
        counted(
            count(&[DiagnosticSeverity::Warning]),
            "avertissement",
            "avertissements",
        ),
        counted(
            count(&[DiagnosticSeverity::Information, DiagnosticSeverity::Hint]),
            "remarque",
            "remarques",
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Ramène les anomalies calculées pour `old_text` sur `new_text` : celles des lignes inchangées au début et à
/// la fin du texte restent (les dernières suivent le décalage de lignes), celles des lignes touchées
/// disparaissent jusqu'au prochain calcul du serveur.
pub fn remap(old_text: &str, new_text: &str, diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    if old_text == new_text {
        return diagnostics.to_vec();
    }
    let old_lines: Vec<&str> = old_text.split('\n').collect();
    let new_lines: Vec<&str> = new_text.split('\n').collect();
    let head = old_lines
        .iter()
        .zip(&new_lines)
        .take_while(|(old, new)| old == new)
        .count();
    let tail = old_lines[head..]
        .iter()
        .rev()
        .zip(new_lines[head..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let tail_start = old_lines.len() - tail;
    let old_end = i64::try_from(old_lines.len()).unwrap_or(i64::MAX);
    let new_end = i64::try_from(new_lines.len()).unwrap_or(i64::MAX);
    let shift = new_end - old_end;
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let (start, end) = (diagnostic.range.start.line, diagnostic.range.end.line);
            let (start, end) = (usize::try_from(start).ok()?, usize::try_from(end).ok()?);
            if end < head {
                return Some(diagnostic.clone());
            }
            if start < tail_start {
                return None;
            }
            let moved = |line: u32| u32::try_from(i64::from(line) + shift).ok();
            let mut shifted = diagnostic.clone();
            shifted.range.start.line = moved(diagnostic.range.start.line)?;
            shifted.range.end.line = moved(diagnostic.range.end.line)?;
            Some(shifted)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::language::{Position, Range};

    fn diagnostic(
        severity: DiagnosticSeverity,
        start: (u32, u32),
        end: (u32, u32),
        message: &str,
    ) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: start.0,
                    character: start.1,
                },
                end: Position {
                    line: end.0,
                    character: end.1,
                },
            },
            severity,
            message: message.to_owned(),
            source: None,
            code: None,
        }
    }

    fn plain(text: &str) -> Vec<CodeToken> {
        vec![CodeToken {
            text: text.to_owned(),
            kind: TokenKind::Plain,
        }]
    }

    fn pieces(line: &[MarkedToken]) -> Vec<(&str, Option<DiagnosticSeverity>)> {
        line.iter()
            .map(|token| (token.text.as_str(), token.mark))
            .collect()
    }

    #[test]
    fn a_token_is_cut_at_the_edges_of_the_range_and_keeps_its_text() {
        let lines = [plain("echo $name")];
        let found = diagnostic(DiagnosticSeverity::Warning, (0, 5), (0, 10), "x");
        let marked = mark_lines(&lines, &[found]);
        assert_eq!(
            pieces(&marked[0]),
            [
                ("echo ", None),
                ("$name", Some(DiagnosticSeverity::Warning))
            ]
        );
        let text: String = marked[0].iter().map(|token| token.text.as_str()).collect();
        assert_eq!(text, "echo $name");
    }

    #[test]
    fn the_kind_of_each_token_is_kept_when_it_is_cut() {
        let line = vec![
            CodeToken {
                text: "echo".to_owned(),
                kind: TokenKind::Keyword,
            },
            CodeToken {
                text: " abc".to_owned(),
                kind: TokenKind::Plain,
            },
        ];
        let found = diagnostic(DiagnosticSeverity::Error, (0, 2), (0, 6), "x");
        let marked = mark_lines(&[line], &[found]);
        let kinds: Vec<TokenKind> = marked[0].iter().map(|token| token.kind).collect();
        let plain_kind = TokenKind::Plain;
        let keyword = TokenKind::Keyword;
        assert_eq!(kinds, [keyword, keyword, plain_kind, plain_kind]);
        let error = Some(DiagnosticSeverity::Error);
        let marks: Vec<Option<DiagnosticSeverity>> =
            marked[0].iter().map(|token| token.mark).collect();
        assert_eq!(marks, [None, error, error, None]);
    }

    #[test]
    fn columns_count_utf16_units() {
        // « 😀 » vaut 2 unités : l'anomalie en colonne 2..3 est le « a » qui suit.
        let lines = [plain("😀ab")];
        let found = diagnostic(DiagnosticSeverity::Error, (0, 2), (0, 3), "x");
        let marked = mark_lines(&lines, &[found]);
        assert_eq!(
            pieces(&marked[0]),
            [
                ("😀", None),
                ("a", Some(DiagnosticSeverity::Error)),
                ("b", None)
            ]
        );
    }

    #[test]
    fn a_range_over_several_lines_marks_the_middle_ones_whole() {
        let lines = [plain("abc"), plain("def"), plain("ghi")];
        let found = diagnostic(DiagnosticSeverity::Error, (0, 1), (2, 2), "x");
        let marked = mark_lines(&lines, &[found]);
        let error = Some(DiagnosticSeverity::Error);
        assert_eq!(pieces(&marked[0]), [("a", None), ("bc", error)]);
        assert_eq!(pieces(&marked[1]), [("def", error)]);
        assert_eq!(pieces(&marked[2]), [("gh", error), ("i", None)]);
    }

    #[test]
    fn an_empty_range_marks_the_character_it_touches() {
        let found = diagnostic(DiagnosticSeverity::Hint, (0, 1), (0, 1), "x");
        let marked = mark_lines(&[plain("abc")], &[found]);
        assert_eq!(
            pieces(&marked[0]),
            [
                ("a", None),
                ("b", Some(DiagnosticSeverity::Hint)),
                ("c", None)
            ]
        );
    }

    #[test]
    fn the_worst_severity_wins_where_ranges_overlap() {
        let hint = diagnostic(DiagnosticSeverity::Hint, (0, 0), (0, 4), "a");
        let error = diagnostic(DiagnosticSeverity::Error, (0, 2), (0, 4), "b");
        let marked = mark_lines(&[plain("abcd")], &[hint, error]);
        assert_eq!(
            pieces(&marked[0]),
            [
                ("ab", Some(DiagnosticSeverity::Hint)),
                ("cd", Some(DiagnosticSeverity::Error))
            ]
        );
    }

    #[test]
    fn lines_without_a_diagnostic_are_unchanged() {
        let marked = mark_lines(&[plain("abc"), plain("")], &[]);
        assert_eq!(pieces(&marked[0]), [("abc", None)]);
        assert!(marked[1].is_empty());
    }

    #[test]
    fn the_gutter_shows_the_worst_diagnostic_of_each_line() {
        let warning = diagnostic(DiagnosticSeverity::Warning, (1, 0), (1, 2), "variable");
        let error = diagnostic(DiagnosticSeverity::Error, (1, 0), (2, 1), "syntaxe");
        let marks = gutter_marks(4, &[warning, error]);
        assert_eq!(marks[0], None);
        let expected = Some(GutterMark {
            severity: DiagnosticSeverity::Error,
            message: "syntaxe".to_owned(),
        });
        assert_eq!(marks[1], expected);
        assert_eq!(marks[2], expected);
        assert_eq!(marks[3], None);
    }

    #[test]
    fn a_diagnostic_past_the_last_line_is_ignored_by_the_gutter() {
        let found = diagnostic(DiagnosticSeverity::Error, (9, 0), (9, 1), "x");
        assert_eq!(gutter_marks(2, &[found]), [None, None]);
    }

    #[test]
    fn the_summary_counts_by_kind_and_stays_silent_without_diagnostics() {
        assert_eq!(summary(&[]), None);
        let one_error = diagnostic(DiagnosticSeverity::Error, (0, 0), (0, 1), "x");
        let single = summary(std::slice::from_ref(&one_error));
        assert_eq!(single.as_deref(), Some("1 erreur"));
        let many = [
            one_error.clone(),
            one_error,
            diagnostic(DiagnosticSeverity::Warning, (0, 0), (0, 1), "x"),
            diagnostic(DiagnosticSeverity::Hint, (0, 0), (0, 1), "x"),
            diagnostic(DiagnosticSeverity::Information, (0, 0), (0, 1), "x"),
        ];
        assert_eq!(
            summary(&many).as_deref(),
            Some("2 erreurs · 1 avertissement · 2 remarques")
        );
    }

    #[test]
    fn every_severity_has_its_own_class() {
        let classes: std::collections::BTreeSet<&str> = [
            DiagnosticSeverity::Error,
            DiagnosticSeverity::Warning,
            DiagnosticSeverity::Information,
            DiagnosticSeverity::Hint,
        ]
        .into_iter()
        .map(mark_class)
        .collect();
        assert_eq!(classes.len(), 4);
    }

    #[test]
    fn diagnostics_stay_as_they_are_for_the_text_they_were_computed_for() {
        let found = diagnostic(DiagnosticSeverity::Error, (1, 0), (1, 1), "x");
        let kept = remap("a\nb", "a\nb", std::slice::from_ref(&found));
        assert_eq!(kept, [found]);
    }

    #[test]
    fn a_diagnostic_on_an_edited_line_disappears_until_the_server_answers() {
        let found = diagnostic(DiagnosticSeverity::Error, (1, 0), (1, 1), "x");
        assert!(remap("a\nb\nc", "a\nbx\nc", &[found]).is_empty());
    }

    #[test]
    fn diagnostics_above_and_below_an_edit_are_kept_and_the_lower_ones_follow_the_lines() {
        let above = diagnostic(DiagnosticSeverity::Error, (0, 0), (0, 1), "haut");
        let below = diagnostic(DiagnosticSeverity::Warning, (2, 0), (2, 1), "bas");
        let kept = remap("a\nb\nc", "a\nb\nnouvelle\nc", &[above.clone(), below]);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0], above);
        assert_eq!(kept[1].range.start.line, 3);
        assert_eq!(kept[1].range.end.line, 3);
        assert_eq!(kept[1].message, "bas");
    }

    #[test]
    fn removing_lines_moves_the_lower_diagnostics_up() {
        let below = diagnostic(DiagnosticSeverity::Warning, (3, 0), (3, 1), "bas");
        let kept = remap("a\nb\nc\nd", "a\nd", &[below]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].range.start.line, 1);
    }
}
