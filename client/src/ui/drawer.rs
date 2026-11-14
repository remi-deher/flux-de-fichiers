//! Tiroir d'inspection : le gabarit `inspection-drawer` du thème, piloté par un signal.

use super::icon::{Icon, IconName};
use leptos::ev;
use leptos::prelude::*;

/// Tiroir latéral d'un dossier (fiche, formulaire). `tabs` vide : un seul volet, sans onglets.
#[component]
pub fn Drawer(
    /// Ouvert tant que ce signal est vrai ; fermer le remet à faux.
    open: RwSignal<bool>,
    eyebrow: &'static str,
    #[prop(into)] title: Signal<String>,
    #[prop(optional)] tabs: Vec<&'static str>,
    #[prop(optional)] tab: Option<RwSignal<usize>>,
    #[prop(optional)] actions: Option<AnyView>,
    children: ChildrenFn,
) -> impl IntoView {
    let tab = tab.unwrap_or_else(|| RwSignal::new(0));
    let close = move || open.set(false);
    let _escape = window_event_listener(ev::keydown, move |event| {
        if event.key() == "Escape" && open.get_untracked() {
            close();
        }
    });
    let has_tabs = tabs.len() > 1;
    view! {
        <aside class="inspection-drawer" class:is-open=move || open.get() aria-hidden=move || (!open.get()).to_string()>
            <button type="button" class="inspection-drawer__scrim" aria-label="Fermer le tiroir" on:click=move |_| close()></button>
            <div class="inspection-drawer__panel" role="dialog" aria-modal="false" tabindex="-1">
                <header class="inspection-drawer__header">
                    <div class="inspection-drawer__identity">
                        <p class="inspection-drawer__eyebrow">{eyebrow}</p>
                        <p class="inspection-panel__title">{move || title.get()}</p>
                    </div>
                    {actions}
                    <button type="button" class="header-icon-btn" aria-label="Fermer le tiroir" on:click=move |_| close()>
                        <Icon name=IconName::Close size=18 />
                    </button>
                </header>
                {has_tabs.then(|| view! {
                    <div class="inspection-drawer__subnav" role="tablist" aria-label="Sections du dossier">
                        {tabs.iter().enumerate().map(|(index, label)| view! {
                            <button type="button" role="tab" class="inspection-drawer__tab"
                                class:is-active=move || tab.get() == index
                                aria-selected=move || (tab.get() == index).to_string()
                                on:click=move |_| tab.set(index)>
                                {*label}
                            </button>
                        }).collect_view()}
                    </div>
                })}
                <div class="inspection-drawer__body">
                    <section class="inspection-drawer__pane" role="tabpanel">
                        {move || open.get().then(|| children())}
                    </section>
                </div>
            </div>
        </aside>
    }
}
