//! Onglet Scripts de la Librairie : liste des scripts locaux. L'écriture se fait dans l'éditeur en
//! pleine page (ADR 0030), ouvert depuis « Écrire un script » ou la carte d'un script.

use super::{LocalDeleteButton, use_local_delete};
use crate::live::Loaded;
use crate::route::{Route, use_router};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
};
use forgearr_shared::local::{LocalLibrarySnapshot, LocalScriptSummary};
use leptos::prelude::*;

#[component]
pub fn ScriptsSection(snapshot: Loaded<LocalLibrarySnapshot>, can_manage: bool) -> impl IntoView {
    let router = use_router();
    let remove = use_local_delete("Script supprimé.");

    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Écrire un script",
            on_click: Callback::new(move |()| router.go(Route::NewScript)),
        });
    }

    let stats = Signal::derive(move || {
        let count = snapshot
            .get()
            .and_then(Result::ok)
            .map_or(0, |s| s.scripts.len());
        vec![StatItem::new(
            "Scripts locaux",
            count.to_string(),
            if count > 0 {
                MetricTone::Success
            } else {
                MetricTone::Neutral
            },
        )]
    });

    view! {
        <StatStrip aria_label="Synthèse des scripts locaux" items=stats />
        {move || {
            let text = query();
            match snapshot.get() {
                None => view! {
                    <div class="empty-state-panel"><p class="text-dim">"Chargement des scripts…"</p></div>
                }.into_any(),
                Some(Err(err)) => view! {
                    <div class="empty-state-panel"><p class="form-error">{err.message()}</p></div>
                }.into_any(),
                Some(Ok(data)) => {
                    let shown: Vec<LocalScriptSummary> = data
                        .scripts
                        .into_iter()
                        .filter(|s| matches(&[&s.name, &s.id, s.interpreter.executable()], &text))
                        .collect();
                    if shown.is_empty() {
                        return view! {
                            <div class="empty-state-panel">
                                <EmptyState
                                    title="Aucun script local"
                                    message="Écrivez un script Bash, Python ou Node.js directement ici : il apparaîtra aussitôt dans la palette de vos flux."
                                    icon_name=IconName::Command
                                />
                            </div>
                        }.into_any();
                    }
                    view! {
                        <div class="resource-grid resource-grid--list">
                            {shown.into_iter().map(|script| {
                                let open_edit = Callback::new({
                                    let target = Route::ScriptEditor(script.id.clone());
                                    move |()| router.go(target.clone())
                                });
                                let badge = view! {
                                    <Tag label=script.interpreter.executable() tone=TagTone::Primary />
                                }.into_any();
                                let actions = can_manage.then(|| {
                                    let del_id = script.id.clone();
                                    view! {
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                open_edit.run(());
                                            }>"Éditer"</button>
                                        <LocalDeleteButton id=del_id remove=remove />
                                    }.into_any()
                                });
                                view! {
                                    <ResourceCard
                                        focus_key=script.id.clone()
                                        title=script.name
                                        subtitle=script.description
                                        icon=IconName::Command
                                        tone=MetricTone::Success
                                        badge=Some(badge)
                                        active=None
                                        on_click=Some(open_edit)
                                        footer_meta=format!("{} · {}", script.id, script.entrypoint)
                                        footer_actions=actions
                                    >
                                        <div class="cluster">
                                            <Tag label=format!("block.{}", script.id) tone=TagTone::Mono />
                                            {script.category.filter(|c| c.shows_badge()).map(|c| view! {
                                                <Tag label=c.as_str() tone=TagTone::Primary />
                                            })}
                                            {script.tags.into_iter().map(|t| view! {
                                                <Tag label=t tone=TagTone::Neutral />
                                            }).collect_view()}
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }
        }}
    }
}
