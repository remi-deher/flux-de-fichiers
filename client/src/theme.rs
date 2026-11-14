//! Thème (Auto, Clair, Sombre) : attribut `data-theme` de la page, choix gardé dans le navigateur.

use crate::prefs;
use leptos::prelude::*;

/// Clé de stockage partagée avec l'interface historique.
const STORAGE_KEY: &str = "flux_theme_pref";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Auto",
            Self::Light => "Clair",
            Self::Dark => "Sombre",
        }
    }

    pub fn parse(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.as_str() == value)
            .unwrap_or(Self::System)
    }
}

/// Applique le thème à la page et le retient.
pub fn apply(theme: Theme) {
    if let Some(root) = document().document_element() {
        let _ = match theme {
            Theme::System => root.remove_attribute("data-theme"),
            other => root.set_attribute("data-theme", other.as_str()),
        };
    }
    prefs::store(STORAGE_KEY, theme.as_str());
}

/// Thème retenu, appliqué au démarrage.
pub fn restore() -> Theme {
    let saved = prefs::load(STORAGE_KEY).map_or(Theme::System, |value| Theme::parse(&value));
    apply(saved);
    saved
}

#[component]
pub fn ThemePicker() -> impl IntoView {
    let current = RwSignal::new(restore());
    view! {
        <div class="theme-segmented" role="group" aria-label="Thème de couleur">
            {Theme::ALL.into_iter().map(|theme| view! {
                <button type="button" class="theme-segmented__btn"
                    aria-pressed=move || (current.get() == theme).to_string()
                    on:click=move |_| { apply(theme); current.set(theme); }>
                    {theme.label()}
                </button>
            }).collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_values_parse_back_and_unknown_ones_follow_the_system() {
        for theme in Theme::ALL {
            assert_eq!(Theme::parse(theme.as_str()), theme);
        }
        assert_eq!(Theme::parse("violet"), Theme::System);
    }
}
