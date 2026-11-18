//! Tiroir d'inspection : le gabarit `inspection-drawer` du thème, piloté par un signal.

use super::icon::{Icon, IconName};
use leptos::ev;
use leptos::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static DRAWER_SEQ: AtomicU64 = AtomicU64::new(1);

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
    let panel_ref = NodeRef::<leptos::html::Div>::new();
    let title_id = format!(
        "drawer-title-{}",
        DRAWER_SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let label_id = title_id.clone();
    let close = move || open.set(false);
    let _escape = window_event_listener(ev::keydown, move |event| {
        if event.key() == "Escape" && open.get_untracked() {
            close();
        }
    });
    Effect::new(move |_| {
        if open.get()
            && let Some(panel) = panel_ref.get()
        {
            let _ = panel.focus();
        }
    });
    let has_tabs = tabs.len() > 1;
    view! {
        <aside class="inspection-drawer" class:is-open=move || open.get() aria-hidden=move || (!open.get()).to_string()>
            <button type="button" class="inspection-drawer__scrim" aria-label="Fermer le tiroir" on:click=move |_| close()></button>
            <div
                node_ref=panel_ref
                class="inspection-drawer__panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby=label_id
                tabindex="-1"
            >
                <header class="inspection-drawer__header">
                    <div class="inspection-drawer__identity">
                        <span class="inspection-drawer__icon" aria-hidden="true">
                            <Icon name=IconName::Layout size=18 />
                        </span>
                        <div class="inspection-drawer__title-group">
                            <span class="inspection-drawer__eyebrow">{eyebrow}</span>
                            <p id=title_id class="inspection-panel__title">{move || title.get()}</p>
                        </div>
                    </div>
                    <div class="inspection-drawer__header-actions">
                        {actions}
                        <button
                            type="button"
                            class="btn btn-secondary btn-sm inspection-drawer__close"
                            aria-label="Fermer le tiroir"
                            title="Fermer (Échap)"
                            on:click=move |_| close()
                        >
                            "Fermer"
                        </button>
                    </div>
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
