//! Sous-navigation d'un écran (`page-subnav`) : onglets vectoriels et compteurs dynamiques.

use super::icon::{Icon, IconName};
use leptos::prelude::*;

const SUBTAB_ICON_SIZE: u32 = 15;

/// Onglet : libellé, icône vectorielle facultative et compteur optionnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubTab {
    pub label: &'static str,
    pub icon: Option<IconName>,
    pub badge: Option<usize>,
}

impl SubTab {
    pub const fn new(label: &'static str, icon: IconName) -> Self {
        Self {
            label,
            icon: Some(icon),
            badge: None,
        }
    }

    pub const fn with_badge(mut self, count: usize) -> Self {
        self.badge = Some(count);
        self
    }
}

/// Rangée d'onglets sélectionnables, commune à la sous-navigation et au tableau à onglets.
#[component]
pub fn SubTabList(
    #[prop(into)] tabs: Signal<Vec<SubTab>>,
    selected: RwSignal<usize>,
) -> impl IntoView {
    view! {
        <div class="app-subnav__scroller subnav-tabs" role="tablist">
            {move || {
                tabs.get()
                    .into_iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        view! {
                            <button type="button" role="tab" class="app-subnav__item subnav-tab"
                                class:active=move || selected.get() == index
                                aria-selected=move || (selected.get() == index).to_string()
                                on:click=move |_| selected.set(index)>
                                {tab.icon.map(|icon| view! {
                                    <Icon name=icon size=SUBTAB_ICON_SIZE class="app-subnav__icon subnav-tab-icon" />
                                })}
                                <span class="app-subnav__label subnav-tab-label">{tab.label}</span>
                                {tab.badge.map(|count| view! {
                                    <span class="subnav-tab__badge">{count}</span>
                                })}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}

#[component]
pub fn SubNav(
    aria_label: &'static str,
    #[prop(into)] tabs: Signal<Vec<SubTab>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtab_carries_vector_icon_and_optional_badge() {
        let tab = SubTab::new("File en direct", IconName::Activity).with_badge(4);
        assert_eq!(tab.label, "File en direct");
        assert_eq!(tab.icon, Some(IconName::Activity));
        assert_eq!(tab.badge, Some(4));
    }
}
