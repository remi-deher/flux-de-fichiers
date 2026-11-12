//! Registre des widgets : ce que chaque page affiche, déclaré en un seul endroit.
//!
//! Le rendu de chaque type est assuré côté client (`client/widgets.rs`), d'après
//! `data-widget`. Ajouter un widget : une variante de `WidgetKind`, une ligne de `WIDGETS`,
//! un rendu dans le script.

use crate::core::panel::{FILES, JOBS, PLUGINS, Panel, WORKERS};
use crate::shared::ui::{Button, ButtonVariant, IconName, Widget, WidgetGrid, WidgetSpan};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Type de widget ; la valeur sérialisée est la clé de son rendu côté client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetKind {
    Attention,
    Running,
    Activity,
    Fleet,
    Libraries,
    Tools,
}

impl WidgetKind {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Running => "running",
            Self::Activity => "activity",
            Self::Fleet => "fleet",
            Self::Libraries => "libraries",
            Self::Tools => "tools",
        }
    }
}

/// Zone de l'application qui accueille une grille de widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetPlacement {
    Dashboard,
    Activity,
    Workers,
}

impl WidgetPlacement {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Activity => "activity",
            Self::Workers => "workers",
        }
    }

    /// Page du catalogue qui accueille la grille (`dashboard` n'a pas de cible de navigation propre).
    pub const fn panel_slug(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Activity => JOBS.slug,
            Self::Workers => WORKERS.slug,
        }
    }
}

/// Déclaration d'un widget : titre, aspect, pages d'accueil et lien « tout voir ».
pub struct WidgetDef {
    pub kind: WidgetKind,
    pub title: &'static str,
    pub icon: IconName,
    pub span: WidgetSpan,
    pub placements: &'static [WidgetPlacement],
    /// Page ouverte par le lien d'en-tête ; omis sur la page qui l'accueille déjà.
    pub goto: Option<(&'static str, Panel)>,
}

pub const WIDGETS: &[WidgetDef] = &[
    WidgetDef {
        kind: WidgetKind::Attention,
        title: "À surveiller",
        icon: IconName::Alert,
        span: WidgetSpan::Medium,
        placements: &[WidgetPlacement::Dashboard],
        goto: None,
    },
    WidgetDef {
        kind: WidgetKind::Running,
        title: "En cours",
        icon: IconName::Activity,
        span: WidgetSpan::Medium,
        placements: &[WidgetPlacement::Dashboard, WidgetPlacement::Activity],
        goto: Some(("Activité", JOBS)),
    },
    WidgetDef {
        kind: WidgetKind::Activity,
        title: "Activité sur 24 h",
        icon: IconName::Activity,
        span: WidgetSpan::Large,
        placements: &[WidgetPlacement::Dashboard, WidgetPlacement::Activity],
        goto: Some(("Activité", JOBS)),
    },
    WidgetDef {
        kind: WidgetKind::Fleet,
        title: "Parc de nœuds",
        icon: IconName::Workers,
        span: WidgetSpan::Medium,
        placements: &[WidgetPlacement::Dashboard],
        goto: Some(("Nœuds", WORKERS)),
    },
    WidgetDef {
        kind: WidgetKind::Libraries,
        title: "Bibliothèques",
        icon: IconName::Libraries,
        span: WidgetSpan::Medium,
        placements: &[WidgetPlacement::Dashboard],
        goto: Some(("Bibliothèques", FILES)),
    },
    WidgetDef {
        kind: WidgetKind::Tools,
        title: "Outils",
        icon: IconName::Plugins,
        span: WidgetSpan::Large,
        placements: &[WidgetPlacement::Workers],
        goto: Some(("Plugins", PLUGINS)),
    },
];

/// Widgets déclarés pour une zone, dans l'ordre du registre.
pub fn widgets_for(placement: WidgetPlacement) -> impl Iterator<Item = &'static WidgetDef> {
    WIDGETS
        .iter()
        .filter(move |def| def.placements.contains(&placement))
}

/// Identifiant DOM d'un widget : un même type peut figurer sur plusieurs pages.
fn widget_dom_id(placement: WidgetPlacement, kind: WidgetKind) -> String {
    format!("widget-{}-{}", placement.as_attr(), kind.as_attr())
}

/// Grille des widgets d'une zone, construite depuis le registre.
#[component]
pub fn WidgetArea(placement: WidgetPlacement) -> impl IntoView {
    view! {
        <WidgetGrid>
            {widgets_for(placement)
                .map(|def| {
                    let dom_id = widget_dom_id(placement, def.kind);
                    let actions = def
                        .goto
                        .filter(|&(_, panel)| panel.slug != placement.panel_slug())
                        .map(|(label, panel)| {
                            view! {
                                <Button variant=ButtonVariant::Secondary class="btn-sm" data_goto=panel.slug>
                                    {label}" →"
                                </Button>
                            }
                            .into_any()
                        });
                    view! {
                        <Widget
                            kind=def.kind.as_attr()
                            id=dom_id
                            title=def.title
                            icon=def.icon
                            span=def.span
                            actions=actions
                        />
                    }
                })
                .collect_view()}
        </WidgetGrid>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const ALL_KINDS: [WidgetKind; 6] = [
        WidgetKind::Attention,
        WidgetKind::Running,
        WidgetKind::Activity,
        WidgetKind::Fleet,
        WidgetKind::Libraries,
        WidgetKind::Tools,
    ];

    #[test]
    fn test_kind_attributes_match_serde_names() {
        for kind in ALL_KINDS {
            let serialized = serde_json::to_string(&kind).unwrap_or_default();
            assert_eq!(serialized, format!("\"{}\"", kind.as_attr()));
        }
        for placement in [
            WidgetPlacement::Dashboard,
            WidgetPlacement::Activity,
            WidgetPlacement::Workers,
        ] {
            let serialized = serde_json::to_string(&placement).unwrap_or_default();
            assert_eq!(serialized, format!("\"{}\"", placement.as_attr()));
        }
    }

    #[test]
    fn test_every_kind_is_declared_exactly_once() {
        let declared: Vec<_> = WIDGETS.iter().map(|def| def.kind).collect();
        let unique: HashSet<_> = declared.iter().map(|kind| kind.as_attr()).collect();
        assert_eq!(unique.len(), declared.len());
        for kind in ALL_KINDS {
            assert!(declared.contains(&kind), "{kind:?} non déclaré");
        }
    }

    #[test]
    fn test_every_kind_has_a_client_renderer() {
        let script = crate::client::get_client_script();
        for kind in ALL_KINDS {
            assert!(
                script.contains(&format!("{}: renderWidget", kind.as_attr())),
                "rendu client manquant pour {}",
                kind.as_attr()
            );
        }
    }

    #[test]
    fn test_every_placement_has_widgets() {
        for placement in [
            WidgetPlacement::Dashboard,
            WidgetPlacement::Activity,
            WidgetPlacement::Workers,
        ] {
            assert!(widgets_for(placement).count() > 0);
        }
    }

    #[test]
    fn test_area_renders_only_the_widgets_of_its_placement() {
        let html = view! { <WidgetArea placement=WidgetPlacement::Activity /> }.to_html();
        assert!(html.contains("data-widget=\"running\""));
        assert!(html.contains("data-widget=\"activity\""));
        assert!(!html.contains("data-widget=\"fleet\""));
        assert!(!html.contains("data-widget=\"attention\""));
    }

    #[test]
    fn test_dom_ids_are_unique_per_placement_and_kind() {
        let html = view! { <WidgetArea placement=WidgetPlacement::Dashboard /> }.to_html();
        assert_eq!(html.matches("data-widget=\"").count(), 5);
        assert!(html.contains("id=\"widget-dashboard-attention\""));
    }

    #[test]
    fn test_goto_link_is_omitted_on_the_page_that_hosts_the_grid() {
        let on_activity = view! { <WidgetArea placement=WidgetPlacement::Activity /> }.to_html();
        assert!(!on_activity.contains("data-goto=\"jobs\""));
        let on_dashboard = view! { <WidgetArea placement=WidgetPlacement::Dashboard /> }.to_html();
        assert!(on_dashboard.contains("data-goto=\"jobs\""));
        assert!(on_dashboard.contains("data-goto=\"workers\""));
    }
}
