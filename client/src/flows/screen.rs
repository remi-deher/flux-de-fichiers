//! Écran Flux (ADR 0035, lot 3) : trois onglets sous `/flows` — Flux, Modèles et Scripts.

use super::{FlowsScope, FlowsSection};
use crate::libraries::ScriptsPanel;
use crate::ui::{PageSubnav, vocabulary};
use crate::url_state::{UrlTab, use_tab};
use forgearr_shared::session::Me;
use leptos::prelude::*;

/// Onglets de l'écran Flux ; le premier est l'onglet par défaut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowsTab {
    #[default]
    Flows,
    Templates,
    Scripts,
}

impl FlowsTab {
    pub const ALL: [Self; 3] = [Self::Flows, Self::Templates, Self::Scripts];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Flows => vocabulary::FLOWS,
            Self::Templates => "Modèles",
            Self::Scripts => "Scripts",
        }
    }
}

impl UrlTab for FlowsTab {
    fn slug(self) -> &'static str {
        match self {
            Self::Flows => "flux",
            Self::Templates => "modeles",
            Self::Scripts => "scripts",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.slug() == slug)
    }
}

#[component]
pub fn FlowsPage(me: Me) -> impl IntoView {
    let active_tab = use_tab(FlowsTab::default(), FlowsTab::ALL.to_vec());
    view! {
        <div class="tab-panel" role="tabpanel">
            <PageSubnav
                aria_label="Sections des flux"
                tabs=FlowsTab::ALL.into_iter().map(|tab| (tab, tab.label())).collect()
                active=active_tab
            />
            {move || match active_tab.get() {
                FlowsTab::Flows => {
                    view! { <FlowsSection me=me.clone() scope=FlowsScope::Flows /> }.into_any()
                }
                FlowsTab::Templates => {
                    view! { <FlowsSection me=me.clone() scope=FlowsScope::Templates /> }.into_any()
                }
                FlowsTab::Scripts => view! { <ScriptsPanel me=me.clone() /> }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flows_tabs_are_named_in_the_address_and_read_back() {
        for tab in FlowsTab::ALL {
            assert_eq!(FlowsTab::from_slug(tab.slug()), Some(tab));
        }
        assert_eq!(FlowsTab::from_slug("inconnu"), None);
        assert_eq!(FlowsTab::default().slug(), "flux");
    }

    #[test]
    fn the_flows_tabs_use_the_shared_vocabulary() {
        assert_eq!(FlowsTab::Flows.label(), "Flux");
        assert_eq!(FlowsTab::Templates.label(), "Modèles");
        assert_eq!(FlowsTab::Scripts.label(), "Scripts");
    }
}
