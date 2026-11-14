//! Versions figées d'un flux : liste, aperçu et restauration dans l'éditeur (ADR 0012).
//!
//! Restaurer remplace le graphe et les métadonnées éditables par ceux de la version ; rien n'est
//! écrit avant « Enregistrer », qui crée alors une nouvelle version. Les en-têtes secrets restent
//! masqués : laissés vides, ils gardent la valeur du flux courant.

use super::state::Editor;
use crate::route::encode_segment;
use crate::ui::{Registre, RegistreMessage};
use crate::{api, format};
use flux_shared::flows::{FlowGraph, FlowRecord, FlowVersionSummary};
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";

/// Applique une version au flux ouvert : graphe, nom, description et masque ; l'identifiant,
/// le statut par défaut et le numéro de version courant sont gardés.
pub fn restore_into(current: &mut FlowRecord, version: &FlowRecord) -> FlowGraph {
    current.name = version.name.clone();
    current.description = version.description.clone();
    current.file_mask = version.file_mask.clone();
    FlowGraph::parse(&version.definition)
}

#[component]
pub fn FlowVersions(
    flow_id: String,
    current: i64,
    editor: Editor,
    meta: RwSignal<FlowRecord>,
    on_restored: Callback<i64>,
) -> impl IntoView {
    let base = format!("{FLOWS_PATH}/{}/versions", encode_segment(&flow_id));
    let list_path = base.clone();
    let versions = LocalResource::new(move || {
        let path = list_path.clone();
        async move { api::get::<Vec<FlowVersionSummary>>(&path).await }
    });
    let feedback = RwSignal::new(None::<String>);
    let restore = Action::new_local(move |version: &i64| {
        let path = format!("{base}/{version}");
        async move { api::get::<FlowRecord>(&path).await }
    });
    Effect::new(move |_| match restore.value().get() {
        Some(Ok(version)) => {
            let mut graph = FlowGraph::default();
            meta.update(|record| graph = restore_into(record, &version));
            editor.edit(|current| *current = graph);
            editor.selected.set(None);
            on_restored.run(version.version);
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    const HEADERS: [&str; 4] = ["Version", "Nom", "Enregistrée", "Action"];
    view! {
        <div class="inspection-panel">
            <p class="form-help">
                "Restaurer charge la version dans l'éditeur ; « Enregistrer » la publie comme nouvelle version. Les versions passées ne changent jamais."
            </p>
            <Registre aria_label="Versions du flux" headers=HEADERS.to_vec()>
                {move || match versions.get() {
                    None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des versions…" /> }.into_any(),
                    Some(Err(error)) => view! { <RegistreMessage span=HEADERS.len() text=error.message() /> }.into_any(),
                    Some(Ok(list)) => list.into_iter().map(|version| {
                        let number = version.version;
                        view! {
                            <tr>
                                <td class="text-mono">{format!("v{number}")}{(number == current).then_some(" (courante)")}</td>
                                <td>{version.name}</td>
                                <td class="text-dim">{format::datetime(version.created_at)}</td>
                                <td class="cell-end">
                                    {(number != current).then(|| view! {
                                        <button type="button" class="btn btn-secondary btn-sm" disabled=move || restore.pending().get()
                                            on:click=move |_| { restore.dispatch(number); }>"Restaurer"</button>
                                    })}
                                </td>
                            </tr>
                        }
                    }).collect_view().into_any(),
                }}
            </Registre>
            <p class="form-help text-danger" role="status">{move || feedback.get()}</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn record(name: &str, version: i64, definition: &str, is_default: bool) -> FlowRecord {
        FlowRecord {
            id: "flow.films".into(),
            name: name.into(),
            description: Some(format!("note {version}")),
            file_mask: format!("*.v{version}"),
            is_default,
            definition: definition.into(),
            version,
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            updated_at: DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    #[test]
    fn restoring_takes_the_content_but_keeps_identity_and_default_flag() {
        let mut current = record("Actuel", 3, "{}", true);
        let old = record(
            "Ancien",
            1,
            r#"{"nodes":[{"id":"n1","type":"file-input","name":"E"}],"connections":[]}"#,
            false,
        );
        let graph = restore_into(&mut current, &old);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(current.name, "Ancien");
        assert_eq!(current.file_mask, "*.v1");
        assert_eq!(current.description.as_deref(), Some("note 1"));
        assert_eq!(current.version, 3);
        assert!(current.is_default);
        assert_eq!(current.id, "flow.films");
    }
}
