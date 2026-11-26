//! Écran de départ d'un nouveau flux : choisir un squelette ou un modèle, puis nom et types de fichiers.

mod cards;
mod skeleton;

use super::editor::FlowEditor;
use super::template_creation::create_flow_from_template;
use super::{FlowPipelineSection, LOCAL_PATH};
use crate::api;
use crate::route::{Link, Route, use_router};
use crate::ui::Toasts;
use crate::ui::file_types_input::{ALL_FILES, FileTypesInput};
use cards::{StarterCard, StarterKey, default_name, group_cards, skeleton_cards, template_cards};
use forgearr_shared::local::{LocalFlowTemplate, LocalLibrarySnapshot};
use forgearr_shared::session::Me;
use leptos::prelude::*;
pub use skeleton::Skeleton;

const NAME_INPUT_ID: &str = "flow-start-name";
const FILE_TYPES_INPUT_ID: &str = "flow-start-file-types";

/// Choix fait sur l'écran de départ, repris par l'éditeur pour un flux pas encore enregistré.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowDraft {
    pub name: String,
    pub file_mask: String,
    pub skeleton: Skeleton,
}

impl Default for FlowDraft {
    fn default() -> Self {
        let skeleton = Skeleton::default();
        Self {
            name: skeleton.default_name().to_owned(),
            file_mask: ALL_FILES.to_owned(),
            skeleton,
        }
    }
}

/// Route « Nouveau flux » : l'écran de départ, puis l'éditeur une fois le départ choisi.
#[component]
pub fn NewFlow(me: Me) -> impl IntoView {
    let draft = RwSignal::new(None::<FlowDraft>);
    view! {
        {move || match draft.get() {
            Some(chosen) => view! {
                <FlowEditor me=me.clone() flow_id=None draft=chosen />
            }.into_any(),
            None => view! {
                <FlowStart on_draft=Callback::new(move |chosen| draft.set(Some(chosen))) />
            }.into_any(),
        }}
    }
}

#[component]
fn FlowStart(on_draft: Callback<FlowDraft>) -> impl IntoView {
    let router = use_router();
    let snapshot = LocalResource::new(|| api::get::<LocalLibrarySnapshot>(LOCAL_PATH));
    let templates = Memo::new(move |_| {
        snapshot
            .get()
            .and_then(Result::ok)
            .map(|loaded| loaded.flow_templates)
            .unwrap_or_default()
    });
    let cards = Memo::new(move |_| {
        let mut all = skeleton_cards();
        all.extend(template_cards(&templates.get()));
        all
    });

    let initial = FlowDraft::default();
    let selected = RwSignal::new(StarterKey::Skeleton(initial.skeleton));
    let name = RwSignal::new(initial.name);
    let file_types = RwSignal::new(initial.file_mask);
    let pick = Callback::new(move |card: StarterCard| {
        name.set(default_name(&card));
        file_types.set(card.file_types.clone());
        selected.set(card.key);
    });
    let selected_template = Memo::new(move |_| match selected.get() {
        StarterKey::Template(id) => templates
            .get()
            .into_iter()
            .find(|template| template.id == id),
        StarterKey::Skeleton(_) => None,
    });

    let create = Action::new_local(
        move |(template, chosen_name, mask): &(LocalFlowTemplate, String, String)| {
            let template = template.clone();
            let chosen_name = chosen_name.clone();
            let mask = mask.clone();
            async move { create_flow_from_template(&template, &chosen_name, &mask).await }
        },
    );
    Effect::new(move |_| match create.value().get() {
        Some(Ok(created)) => {
            Toasts::notify_success("Flux créé depuis le modèle.");
            router.go(Route::FlowEditor(created.id));
        }
        Some(Err(error)) => Toasts::notify_error(error.message()),
        None => {}
    });

    let start = move |_| {
        let chosen_name = name.get_untracked().trim().to_owned();
        if chosen_name.is_empty() {
            Toasts::notify_error("Donnez un nom au flux.");
            return;
        }
        let mask = file_types.get_untracked();
        match (selected.get_untracked(), selected_template.get_untracked()) {
            (StarterKey::Skeleton(skeleton), _) => on_draft.run(FlowDraft {
                name: chosen_name,
                file_mask: mask,
                skeleton,
            }),
            (StarterKey::Template(_), Some(template)) => {
                create.dispatch((template, chosen_name, mask));
            }
            (StarterKey::Template(_), None) => {}
        }
    };

    view! {
        <div class="flow-start">
            <header class="flow-start__header">
                <Link to=Route::Flows class="btn btn-secondary btn-sm">"Retour aux flux"</Link>
                <h1 class="flow-start__title">"Nouveau flux"</h1>
                <p class="form-help">"Partez d'un squelette ou d'un modèle, vous pourrez tout modifier ensuite."</p>
            </header>
            <div class="flow-start__body">
                <div class="flow-start__choices">
                    {move || group_cards(&cards.get()).into_iter().map(|(group, members)| view! {
                        <section class="flow-start__group">
                            <h2 class="flow-start__group-title">{group.title()}</h2>
                            <div class="flow-start__grid">
                                {members.into_iter().map(|card| {
                                    let key = card.key.clone();
                                    let title = card.title.clone();
                                    let description = card.description.clone();
                                    view! {
                                        <button type="button" class="flow-start__card"
                                            aria-pressed=move || (selected.get() == key).to_string()
                                            on:click=move |_| pick.run(card.clone())>
                                            <span class="flow-start__card-title">{title}</span>
                                            <span class="flow-start__card-desc">{description}</span>
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </section>
                    }).collect_view()}
                    {move || templates.with(Vec::is_empty).then(|| view! {
                        <p class="form-help">
                            "Les modèles du Store apparaissent ici une fois installés. "
                            <Link to=Route::Plugins>"Ouvrir Extensions"</Link>
                        </p>
                    })}
                </div>
                <aside class="flow-start__side">
                    <div class="form-group">
                        <label for=NAME_INPUT_ID class="form-label">"Nom du flux"</label>
                        <input id=NAME_INPUT_ID type="text" class="form-input"
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event)) />
                    </div>
                    <div class="form-group">
                        <label for=FILE_TYPES_INPUT_ID class="form-label">"Types de fichiers"</label>
                        <FileTypesInput input_id=FILE_TYPES_INPUT_ID value=file_types
                            on_change=Callback::new(move |value| file_types.set(value)) />
                        <p class="form-help">"Les fichiers que ce flux traitera. Modifiable plus tard."</p>
                    </div>
                    {move || match selected_template.get() {
                        Some(template) => view! {
                            <FlowPipelineSection
                                definition=serde_json::to_string(&template.definition).unwrap_or_default()
                                empty_label="Modèle vide" />
                        }.into_any(),
                        None => ().into_any(),
                    }}
                    <button type="button" class="btn btn-primary"
                        disabled=move || create.pending().get()
                        on:click=start>
                        "Créer et ouvrir l'éditeur"
                    </button>
                </aside>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_draft_is_a_blank_flow_for_all_files() {
        let draft = FlowDraft::default();
        assert_eq!(draft.skeleton, Skeleton::Blank);
        assert_eq!(draft.name, "Nouveau flux");
        assert_eq!(draft.file_mask, ALL_FILES);
    }
}
