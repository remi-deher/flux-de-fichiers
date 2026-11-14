//! Sous-navigation d'un écran (`page-subnav`) : onglets et actions de la page.

use leptos::prelude::*;

/// Onglet : libellé et pictogramme facultatif.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubTab {
    pub label: &'static str,
    pub icon: Option<&'static str>,
}

impl SubTab {
    pub const fn new(label: &'static str, icon: &'static str) -> Self {
        Self {
            label,
            icon: Some(icon),
        }
    }
}

/// Rangée d'onglets sélectionnables, commune à la sous-navigation et au tableau à onglets.
#[component]
pub fn SubTabList(tabs: Vec<SubTab>, selected: RwSignal<usize>) -> impl IntoView {
    view! {
        <div class="app-subnav__scroller subnav-tabs" role="tablist">
            {tabs.into_iter().enumerate().map(|(index, tab)| view! {
                <button type="button" role="tab" class="app-subnav__item subnav-tab"
                    class:active=move || selected.get() == index
                    aria-selected=move || (selected.get() == index).to_string()
                    on:click=move |_| selected.set(index)>
                    {tab.icon.map(|icon| view! { <span class="app-subnav__icon subnav-tab-icon" aria-hidden="true">{icon}</span> })}
                    <span class="app-subnav__label subnav-tab-label">{tab.label}</span>
                </button>
            }).collect_view()}
        </div>
    }
}

#[component]
pub fn SubNav(
    aria_label: &'static str,
    tabs: Vec<SubTab>,
    selected: RwSignal<usize>,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <nav class="page-subnav app-subnav-toolbar" aria-label=aria_label>
            <div class="subnav-container app-subnav-container">
                <div class="app-subnav">
                    <SubTabList tabs=tabs selected=selected />
                </div>
                <div class="subnav-actions">{actions}</div>
            </div>
        </nav>
    }
}
