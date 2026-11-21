//! Choix d'un flux du central : sous-flux d'un bloc, flux d'échec du flux et flux d'échec global
//! (ADR 0026).

use super::fields::select_options;
use super::state::Editor;
use crate::api;
use crate::ui::Toasts;
use forgearr_shared::flows::{DefaultFailureFlow, FlowRecord};
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";
const DEFAULT_FAILURE_PATH: &str = "/api/v1/flows/failure-default";
const NO_FLOW_LABEL: &str = "Aucun flux";
const GLOBAL_FAILURE_LABEL: &str = "Flux d'échec global";

/// Options `(identifiant, nom)` des flux, précédées de l'option vide et sans le flux `exclude`.
pub fn options_from(
    empty: &str,
    flows: &[FlowRecord],
    exclude: Option<&str>,
) -> Vec<(String, String)> {
    let mut options = vec![(String::new(), empty.to_owned())];
    options.extend(
        flows
            .iter()
            .filter(|flow| Some(flow.id.as_str()) != exclude)
            .map(|flow| (flow.id.clone(), flow.name.clone())),
    );
    options
}

/// Flux enregistrés sous forme d'options, chargés une fois.
pub(super) fn flow_options(
    empty: &'static str,
    exclude: Option<String>,
) -> Signal<Vec<(String, String)>> {
    let flows = LocalResource::new(|| async {
        api::get::<Vec<FlowRecord>>(FLOWS_PATH)
            .await
            .unwrap_or_default()
    });
    Signal::derive(move || {
        options_from(empty, &flows.get().unwrap_or_default(), exclude.as_deref())
    })
}

/// Liste déroulante d'un flux ; `on_pick` reçoit l'identifiant choisi (vide : aucun).
#[component]
fn FlowSelect(
    input_id: &'static str,
    label: &'static str,
    disabled: bool,
    options: Signal<Vec<(String, String)>>,
    #[prop(into)] value: Signal<String>,
    on_pick: Callback<String>,
) -> impl IntoView {
    view! {
        <div class="form-group">
            <label for=input_id class="form-label">{label}</label>
            <select id=input_id class="form-select" disabled=disabled
                prop:value=move || value.get()
                on:change=move |event| on_pick.run(event_target_value(&event))>
                {move || select_options(&options.get(), &value.get()).into_iter().map(|(id, name)| view! {
                    <option value=id>{name}</option>
                }).collect_view()}
            </select>
        </div>
    }
}

/// Flux d'échec du flux ouvert (clé `failureFlowId` du graphe), puis flux d'échec global.
#[component]
pub fn FailureFlowSettings(editor: Editor, flow_id: String, can_manage: bool) -> impl IntoView {
    let own_id = (!flow_id.is_empty()).then_some(flow_id);
    let options = flow_options(GLOBAL_FAILURE_LABEL, own_id);
    let chosen = Signal::derive(move || {
        editor
            .graph
            .with(|graph| graph.failure_flow_id().unwrap_or_default().to_owned())
    });
    let pick_own = Callback::new(move |id: String| {
        editor.edit(move |graph| graph.set_failure_flow_id(Some(&id)));
    });

    let global_options = flow_options(NO_FLOW_LABEL, None);
    let global = RwSignal::new(String::new());
    let loaded = LocalResource::new(|| async {
        api::get::<DefaultFailureFlow>(DEFAULT_FAILURE_PATH)
            .await
            .unwrap_or_default()
    });
    Effect::new(move || {
        if let Some(current) = loaded.get() {
            global.set(current.flow_id.unwrap_or_default());
        }
    });
    let pick_global = Callback::new(move |id: String| {
        global.set(id.clone());
        leptos::task::spawn_local(async move {
            let body = DefaultFailureFlow {
                flow_id: (!id.is_empty()).then_some(id),
            };
            match api::put::<DefaultFailureFlow, DefaultFailureFlow>(DEFAULT_FAILURE_PATH, &body)
                .await
            {
                Ok(_) => Toasts::notify_success("Flux d'échec global enregistré."),
                Err(_) => Toasts::notify_error("Flux d'échec global refusé."),
            }
        });
    });

    view! {
        <div class="flow-inspector__section">
            <h3 class="flow-inspector__section-title">"Flux d'échec"</h3>
            <FlowSelect input_id="flow-settings-failure" label="Flux d'échec de ce flux"
                disabled=!can_manage options=options value=chosen on_pick=pick_own />
            <p class="form-help">
                "Exécuté sur la tâche quand elle échoue, avec {$error.code} et {$error.message}. "
                "La tâche reste échouée."
            </p>
            <FlowSelect input_id="flow-settings-failure-global" label="Flux d'échec global"
                disabled=!can_manage options=global_options value=global on_pick=pick_global />
            <p class="form-help">"Appliqué aux flux qui n'en désignent aucun ; enregistré aussitôt."</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(id: &str, name: &str) -> FlowRecord {
        FlowRecord {
            id: id.into(),
            name: name.into(),
            description: None,
            file_mask: "*".into(),
            is_default: false,
            definition: "{}".into(),
            version: 1,
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn options_start_empty_and_skip_the_excluded_flow() {
        let flows = [flow("flow.a", "A"), flow("flow.b", "B")];
        let options = options_from(NO_FLOW_LABEL, &flows, Some("flow.a"));
        assert_eq!(
            options,
            vec![
                (String::new(), NO_FLOW_LABEL.to_owned()),
                ("flow.b".to_owned(), "B".to_owned())
            ]
        );
        assert_eq!(options_from(NO_FLOW_LABEL, &flows, None).len(), 3);
    }
}
