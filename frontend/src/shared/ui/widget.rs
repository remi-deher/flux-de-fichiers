//! Kit de widgets : une coquille commune (en-tête, états) et une grille, réutilisables sur toute page.
//!
//! Le contenu d'un widget est rempli par le script client d'après `data-widget` ; la coquille
//! porte l'état (`data-state`) que le thème traduit en squelette, message vide ou erreur.

use super::icon::{Icon, IconName};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Taille de l'icône d'en-tête d'un widget, en pixels.
const WIDGET_ICON_SIZE: u32 = 16;

/// Largeur d'un widget sur la grille de six colonnes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetSpan {
    Small,
    #[default]
    Medium,
    Large,
}

impl WidgetSpan {
    pub const fn as_class(self) -> &'static str {
        match self {
            Self::Small => "widget widget--small",
            Self::Medium => "widget widget--medium",
            Self::Large => "widget widget--large",
        }
    }
}

/// État d'affichage d'un widget ; le script client le fait évoluer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetState {
    #[default]
    Loading,
    Empty,
    Error,
    Ready,
}

impl WidgetState {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Empty => "empty",
            Self::Error => "error",
            Self::Ready => "ready",
        }
    }
}

/// Grille de widgets : six colonnes, repliée en une colonne sur petit écran.
#[component]
pub fn WidgetGrid(children: Children) -> impl IntoView {
    view! { <section class="widget-grid">{children()}</section> }
}

/// Coquille d'un widget. `kind` relie le widget à son rendu côté client.
#[component]
pub fn Widget(
    kind: &'static str,
    #[prop(into)] id: String,
    title: &'static str,
    icon: IconName,
    #[prop(optional)] span: WidgetSpan,
    #[prop(optional_no_strip)] actions: Option<AnyView>,
) -> impl IntoView {
    let title_id = format!("{id}-title");
    view! {
        <section
            id=id.clone()
            class=span.as_class()
            data-widget=kind
            data-state=WidgetState::default().as_attr()
            aria-labelledby=title_id.clone()
        >
            <header class="widget__head">
                <span class="widget__icon"><Icon name=icon size=WIDGET_ICON_SIZE /></span>
                <h3 id=title_id.clone() class="widget__title">{title}</h3>
                {actions.map(|content| view! { <div class="widget__actions">{content}</div> })}
            </header>
            <div class="widget__body" data-widget-body></div>
            <p class="widget__message" data-widget-message role="status"></p>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_and_state_names_match_serde() {
        for span in [WidgetSpan::Small, WidgetSpan::Medium, WidgetSpan::Large] {
            let serialized = serde_json::to_string(&span).unwrap_or_default();
            assert!(span.as_class().ends_with(serialized.trim_matches('"')));
        }
        for state in [
            WidgetState::Loading,
            WidgetState::Empty,
            WidgetState::Error,
            WidgetState::Ready,
        ] {
            let serialized = serde_json::to_string(&state).unwrap_or_default();
            assert_eq!(serialized, format!("\"{}\"", state.as_attr()));
        }
    }

    #[test]
    fn test_widget_starts_loading_and_exposes_its_kind() {
        let html = view! { <Widget kind="demo" id="w-demo" title="Démo" icon=IconName::Check /> }
            .to_html();
        assert!(html.contains("data-widget=\"demo\""));
        assert!(html.contains("data-state=\"loading\""));
        assert!(html.contains("aria-labelledby=\"w-demo-title\""));
        assert!(html.contains("id=\"w-demo-title\""));
        assert!(html.contains("data-widget-body"));
        assert!(html.contains("data-widget-message"));
        assert!(html.contains("widget--medium"));
        assert!(!html.contains("widget__actions"));
    }

    #[test]
    fn test_widget_renders_span_and_actions_slot() {
        let actions = view! { <button>"Voir"</button> }.into_any();
        let html = view! {
            <Widget kind="demo" id="w-demo" title="Démo" icon=IconName::Check span=WidgetSpan::Large actions=Some(actions) />
        }
        .to_html();
        assert!(html.contains("widget--large"));
        assert!(html.contains("widget__actions"));
        assert!(html.contains("Voir"));
    }

    #[test]
    fn test_grid_wraps_its_children() {
        let html = view! { <WidgetGrid><p>"x"</p></WidgetGrid> }.to_html();
        assert!(html.contains("class=\"widget-grid\""));
    }
}
