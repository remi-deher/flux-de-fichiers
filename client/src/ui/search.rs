//! Recherche contextuelle de la barre du haut : chaque écran filtre ses lignes avec ce texte.

use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct Search {
    pub text: RwSignal<String>,
    /// Texte à appliquer au prochain changement d'écran (résultat de la recherche globale).
    pub pending: RwSignal<Option<String>>,
}

impl Search {
    pub fn install() -> Self {
        let search = Self {
            text: RwSignal::new(String::new()),
            pending: RwSignal::new(None),
        };
        provide_context(search);
        search
    }
}

/// Texte recherché (vide hors contexte).
pub fn query() -> String {
    use_context::<Search>()
        .map(|search| search.text.get())
        .unwrap_or_default()
}

/// Une des valeurs contient-elle le texte, sans tenir compte de la casse ?
pub fn matches(values: &[&str], query: &str) -> bool {
    let term = query.trim().to_lowercase();
    term.is_empty()
        || values
            .iter()
            .any(|value| value.to_lowercase().contains(&term))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn matching_ignores_case_and_an_empty_query_keeps_everything() {
        assert!(matches(&["Film.MKV"], " mkv "));
        assert!(!matches(&["Film.MKV"], "zzz"));
        assert!(matches(&[], ""));
    }
}
