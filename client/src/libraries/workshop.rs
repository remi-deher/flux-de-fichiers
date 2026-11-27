//! Écran Atelier (ADR 0035, lot 5) : ce qui sert à bâtir les flux sans être un flux — ressources,
//! modèles de blocs et packs locaux. Deux onglets sous `/workshop`.

use super::packs::LocalPacksSection;
use super::resources::ResourcesSection;
use super::{FLOWS_PATH, LOCAL_PATH, LOCAL_TOPICS};
use crate::api;
use crate::live::load;
use crate::ui::PageSubnav;
use crate::url_state::{UrlTab, use_tab};
use forgearr_shared::Permission;
use forgearr_shared::flows::FlowRecord;
use forgearr_shared::local::LocalLibrarySnapshot;
use forgearr_shared::session::Me;
use leptos::prelude::*;

/// Onglets de l'Atelier ; le premier est l'onglet par défaut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkshopTab {
    #[default]
    Resources,
    Packs,
}

impl WorkshopTab {
    pub const ALL: [Self; 2] = [Self::Resources, Self::Packs];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Resources => "Ressources & Blocs",
            Self::Packs => "Packs",
        }
    }
}

impl UrlTab for WorkshopTab {
    fn slug(self) -> &'static str {
        match self {
            Self::Resources => "resources",
            Self::Packs => "packs",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.slug() == slug)
    }
}

#[component]
pub fn WorkshopPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageSettings);
    let active_tab = use_tab(WorkshopTab::default(), WorkshopTab::ALL.to_vec());
    let flows = LocalResource::new(|| api::get::<Vec<FlowRecord>>(FLOWS_PATH));
    let snapshot = load::<LocalLibrarySnapshot>(LOCAL_TOPICS, LOCAL_PATH);
    view! {
        <div class="tab-panel" role="tabpanel">
            <PageSubnav
                aria_label="Sections de l'atelier"
                tabs=WorkshopTab::ALL.into_iter().map(|tab| (tab, tab.label())).collect()
                active=active_tab
            />
            {move || match active_tab.get() {
                WorkshopTab::Resources => view! {
                    <ResourcesSection snapshot=snapshot flows=flows can_manage=can_manage />
                }.into_any(),
                WorkshopTab::Packs => view! {
                    <LocalPacksSection snapshot=snapshot can_manage=can_manage />
                }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workshop_tabs_are_named_in_the_address_and_read_back() {
        for tab in WorkshopTab::ALL {
            assert_eq!(WorkshopTab::from_slug(tab.slug()), Some(tab));
        }
        assert_eq!(WorkshopTab::from_slug("scripts"), None);
        assert_eq!(WorkshopTab::default().slug(), "resources");
        assert_eq!(WorkshopTab::Packs.label(), "Packs");
    }
}
