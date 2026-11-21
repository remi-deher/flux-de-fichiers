//! Formulaire des variables du script d'un nœud, généré depuis son schéma (ADR 0025, phase D) :
//! champs typés, sélecteur de références disponibles en amont et aperçu de la configuration résolue.

use super::script_inspector::update_node_config;
use super::state::Editor;
use crate::api;
use forgearr_shared::flow_variables::{
    NodeVariablesRequest, NodeVariablesView, ReferenceSource, ReferenceSuggestion,
};
use forgearr_shared::script_variables::{
    SECRETS_CONFIG_KEY, ScriptVariable, VariableKind, default_from_text, default_to_text,
};
use leptos::prelude::*;
use serde_json::{Map, Value};

const NODE_VARIABLES_PATH: &str = "/api/v1/flows/node-variables";
const REFERENCE_OPEN: &str = "{{";
const REFERENCE_CLOSE: &str = "}}";
const BOOLEAN_TRUE: &str = "true";
const BOOLEAN_FALSE: &str = "false";
const SECRET_STORED_HINT: &str = "Secret enregistré — laisser vide pour le conserver";
const SECRET_EMPTY_HINT: &str = "Aucun secret enregistré";
const SECRET_CLEAR_LABEL: &str = "Effacer le secret";

/// Valeur à enregistrer dans la config du nœud d'après le texte saisi ; `None` retire la clé
/// (le défaut du schéma s'applique). Un texte qui contient une référence reste du texte.
pub fn value_from_input(kind: VariableKind, raw: &str) -> Option<Value> {
    if raw.trim().is_empty() {
        return None;
    }
    let typed = match kind {
        VariableKind::Number | VariableKind::Boolean if !raw.contains(REFERENCE_OPEN) => {
            default_from_text(kind, raw).ok().flatten()
        }
        _ => None,
    };
    Some(typed.unwrap_or_else(|| Value::String(raw.to_owned())))
}

/// Le flux porte-t-il une valeur pour ce secret ? (enregistrée, donc masquée par l'API, ou saisie)
pub fn is_secret_stored(value: Option<&Value>) -> bool {
    value.is_some_and(|value| !value.is_null())
}

/// Range un secret saisi dans `config.secrets` ; une saisie vide ne change rien (le secret
/// enregistré est conservé), `Value::Null` demande son effacement à l'enregistrement.
pub fn apply_secret(config: &mut Map<String, Value>, name: &str, value: Value) {
    if value.as_str().is_some_and(str::is_empty) {
        return;
    }
    let secrets = config
        .entry(SECRETS_CONFIG_KEY)
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(object) = secrets.as_object_mut() {
        object.insert(name.to_owned(), value);
    }
}

/// Texte affiché dans le champ pour la valeur enregistrée.
pub fn input_text(value: Option<&Value>) -> String {
    default_to_text(value)
}

/// Ajoute `{{référence}}` au texte courant du champ.
pub fn append_reference(current: &str, reference: &str) -> String {
    format!("{current}{REFERENCE_OPEN}{reference}{REFERENCE_CLOSE}")
}

/// Références regroupées par source, dans l'ordre d'affichage du sélecteur ; les groupes vides sont omis.
pub fn group_references(
    references: &[ReferenceSuggestion],
) -> Vec<(ReferenceSource, Vec<&ReferenceSuggestion>)> {
    [
        ReferenceSource::Flow,
        ReferenceSource::MediaInfo,
        ReferenceSource::Node,
        ReferenceSource::Plugin,
    ]
    .into_iter()
    .map(|source| {
        let group: Vec<&ReferenceSuggestion> = references
            .iter()
            .filter(|suggestion| suggestion.source == source)
            .collect();
        (source, group)
    })
    .filter(|(_, group)| !group.is_empty())
    .collect()
}

/// Libellé d'une option du sélecteur : la référence, son intitulé et sa valeur d'exemple.
fn reference_option_label(suggestion: &ReferenceSuggestion) -> String {
    match &suggestion.sample {
        Some(sample) => format!(
            "{} — {} (ex. {sample})",
            suggestion.reference, suggestion.label
        ),
        None => format!("{} — {}", suggestion.reference, suggestion.label),
    }
}

fn set_variable(editor: Editor, node_id: &str, name: &str, value: Option<Value>) {
    let name = name.to_owned();
    update_node_config(editor, node_id, move |map| match value {
        Some(value) => {
            map.insert(name, value);
        }
        None => {
            map.remove(&name);
        }
    });
}

fn set_secret(editor: Editor, node_id: &str, name: &str, value: Value) {
    let name = name.to_owned();
    update_node_config(editor, node_id, move |map| apply_secret(map, &name, value));
}

fn current_secret(editor: Editor, node_id: &str, name: &str) -> Option<Value> {
    editor.graph.with(|graph| {
        graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .and_then(|node| node.config.get(SECRETS_CONFIG_KEY)?.get(name).cloned())
    })
}

fn current_value(editor: Editor, node_id: &str, name: &str) -> Option<Value> {
    editor.graph.with(|graph| {
        graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .and_then(|node| node.config.get(name).cloned())
    })
}

#[component]
fn ReferencePicker(
    references: Vec<ReferenceSuggestion>,
    on_pick: Callback<String>,
    disabled: bool,
) -> impl IntoView {
    view! {
        <select class="form-input" disabled=disabled aria-label="Insérer une référence"
            prop:value=""
            on:change=move |event| {
                let reference = event_target_value(&event);
                if !reference.is_empty() {
                    on_pick.run(reference);
                }
            }>
            <option value="">"Insérer une référence…"</option>
            {group_references(&references).into_iter().map(|(source, group)| view! {
                <optgroup label=source.label()>
                    {group.into_iter().map(|suggestion| view! {
                        <option value=suggestion.reference.clone()>{reference_option_label(suggestion)}</option>
                    }).collect_view()}
                </optgroup>
            }).collect_view()}
        </select>
    }
}

/// Champ d'un secret : jamais pré-rempli, la valeur saisie n'est pas réaffichée.
#[component]
fn SecretField(
    editor: Editor,
    node_id: String,
    name: String,
    field_id: String,
    read_only: bool,
) -> impl IntoView {
    let typed = RwSignal::new(String::new());
    let stored = {
        let (node_id, name) = (node_id.clone(), name.clone());
        move || is_secret_stored(current_secret(editor, &node_id, &name).as_ref())
    };
    let commit = {
        let (node_id, name) = (node_id.clone(), name.clone());
        move |raw: String| {
            set_secret(editor, &node_id, &name, Value::String(raw));
            typed.set(String::new());
        }
    };
    let clear = move |_| set_secret(editor, &node_id, &name, Value::Null);
    view! {
        <input id=field_id class="form-input" type="password" autocomplete="new-password"
            disabled=read_only
            placeholder=move || if stored() { SECRET_STORED_HINT } else { SECRET_EMPTY_HINT }
            prop:value=move || typed.get()
            on:input=move |event| typed.set(event_target_value(&event))
            on:change=move |event| commit(event_target_value(&event)) />
        {move || stored().then(|| view! {
            <button type="button" class="btn btn-secondary btn-sm" disabled=read_only
                on:click=clear.clone()>{SECRET_CLEAR_LABEL}</button>
        })}
    }
}

#[component]
fn VariableField(
    editor: Editor,
    node_id: String,
    variable: ScriptVariable,
    references: Vec<ReferenceSuggestion>,
    can_manage: bool,
) -> impl IntoView {
    let kind = variable.kind;
    let name = variable.name.clone();
    let field_id = format!("script-var-{}", variable.name);
    let read_only = !can_manage;
    let value_text = {
        let (node_id, name) = (node_id.clone(), name.clone());
        move || input_text(current_value(editor, &node_id, &name).as_ref())
    };
    let commit = {
        let (node_id, name) = (node_id.clone(), name.clone());
        move |raw: String| set_variable(editor, &node_id, &name, value_from_input(kind, &raw))
    };
    let placeholder = default_to_text(variable.default_value.as_ref());
    let control = match kind {
        VariableKind::Boolean => {
            let commit = commit.clone();
            view! {
                <select id=field_id.clone() class="form-input" disabled=read_only
                    prop:value=value_text.clone()
                    on:change=move |event| commit(event_target_value(&event))>
                    <option value="">"Valeur par défaut"</option>
                    <option value=BOOLEAN_TRUE>"Oui"</option>
                    <option value=BOOLEAN_FALSE>"Non"</option>
                </select>
            }
            .into_any()
        }
        VariableKind::Choice => {
            let commit = commit.clone();
            view! {
                <select id=field_id.clone() class="form-input" disabled=read_only
                    prop:value=value_text.clone()
                    on:change=move |event| commit(event_target_value(&event))>
                    <option value="">"Valeur par défaut"</option>
                    {variable.options.iter().map(|option| view! {
                        <option value=option.clone()>{option.clone()}</option>
                    }).collect_view()}
                </select>
            }
            .into_any()
        }
        VariableKind::Secret => view! {
            <SecretField editor=editor node_id=node_id.clone() name=name.clone()
                field_id=field_id.clone() read_only=read_only />
        }
        .into_any(),
        VariableKind::Multiline => {
            let commit = commit.clone();
            view! {
                <textarea id=field_id.clone() class="form-input" rows="3" disabled=read_only
                    placeholder=placeholder.clone()
                    prop:value=value_text.clone()
                    on:change=move |event| commit(event_target_value(&event)) />
            }
            .into_any()
        }
        VariableKind::Text | VariableKind::Number => {
            let commit = commit.clone();
            view! {
                <input id=field_id.clone() class="form-input" disabled=read_only
                    placeholder=placeholder.clone()
                    prop:value=value_text.clone()
                    on:change=move |event| commit(event_target_value(&event)) />
            }
            .into_any()
        }
    };
    let accepts_references = matches!(
        kind,
        VariableKind::Text | VariableKind::Multiline | VariableKind::Number
    ) && !references.is_empty();
    let picker = accepts_references.then(|| {
        let value_text = value_text.clone();
        let commit = commit.clone();
        view! {
            <ReferencePicker
                references=references.clone()
                disabled=read_only
                on_pick=Callback::new(move |reference: String| {
                    commit(append_reference(&value_text(), &reference));
                })
            />
        }
    });
    view! {
        <div class="form-group">
            <label class="form-label" for=field_id>
                {format!("{}{}", variable.display_label(), if variable.required { " *" } else { "" })}
            </label>
            {control}
            {picker}
            {variable.description.clone().map(|text| view! { <p class="form-help">{text}</p> })}
        </div>
    }
}

/// Aperçu de la `config` que le script recevrait, avec des valeurs d'exemple pour les références.
#[component]
fn ResolvedPreview(editor: Editor, node_id: String) -> impl IntoView {
    let preview = LocalResource::new(move || {
        let request = NodeVariablesRequest {
            graph: editor.graph.get(),
            node_id: node_id.clone(),
            media_info: editor
                .sim_input
                .get_untracked()
                .to_probe_value()
                .unwrap_or(Value::Null),
        };
        async move { api::post::<_, NodeVariablesView>(NODE_VARIABLES_PATH, Some(&request)).await }
    });
    view! {
        <div class="form-group">
            <span class="form-label">"Configuration résolue (valeurs d'exemple)"</span>
            {move || match preview.get() {
                None => view! { <p class="text-dim text-sm">"Calcul…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="form-error">{error.message()}</p> }.into_any(),
                Some(Ok(data)) => {
                    let warnings = data.warnings;
                    let body = match (data.error, data.resolved) {
                        (Some(message), _) => view! { <p class="form-error">{message}</p> }.into_any(),
                        (None, Some(resolved)) => view! {
                            <pre class="code-block-wrapper"><code>
                                {serde_json::to_string_pretty(&resolved).unwrap_or_default()}
                            </code></pre>
                        }
                        .into_any(),
                        (None, None) => ().into_any(),
                    };
                    view! {
                        {body}
                        {warnings.into_iter().map(|warning| view! {
                            <p class="form-help">{format!("Attention : {warning}")}</p>
                        }).collect_view()}
                    }
                    .into_any()
                }
            }}
        </div>
    }
}

/// Clé qui ne change que si la structure du flux ou l'exemple de métadonnées change, pas à chaque
/// frappe : le formulaire garde ainsi le focus pendant la saisie.
fn structure_key(editor: Editor, node_id: &str) -> String {
    let media = editor.sim_input.with(|input| input.probe_json.clone());
    editor.graph.with(|graph| {
        let nodes: Vec<String> = graph
            .nodes
            .iter()
            .map(|node| format!("{}|{}|{}", node.id, node.name, node.kind.as_str()))
            .collect();
        let script = graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .and_then(|node| node.config.get("scriptId").and_then(Value::as_str))
            .unwrap_or_default();
        format!(
            "{script}#{}#{:?}#{media}",
            nodes.join(";"),
            graph.connections
        )
    })
}

/// Formulaire des variables du script sélectionné sur le nœud, puis aperçu de la config résolue.
#[component]
pub fn ScriptVariablesPanel(editor: Editor, node_id: String, can_manage: bool) -> impl IntoView {
    let key_node = node_id.clone();
    let structure = Memo::new(move |_| structure_key(editor, &key_node));
    let schema_node = node_id.clone();
    let schema = LocalResource::new(move || {
        structure.track();
        let request = NodeVariablesRequest {
            graph: editor.graph.get_untracked(),
            node_id: schema_node.clone(),
            media_info: editor
                .sim_input
                .get_untracked()
                .to_probe_value()
                .unwrap_or(Value::Null),
        };
        async move { api::post::<_, NodeVariablesView>(NODE_VARIABLES_PATH, Some(&request)).await }
    });
    let form_node = node_id.clone();
    view! {
        {move || {
            let node_id = form_node.clone();
            schema.get().map(|result| match result {
                Err(error) => view! { <p class="form-error">{error.message()}</p> }.into_any(),
                Ok(data) if data.variables.is_empty() => ().into_any(),
                Ok(data) => {
                    let references = data.references;
                    let preview_node = node_id.clone();
                    view! {
                        <div class="form-group">
                            <span class="form-label">"Variables du script"</span>
                        </div>
                        {data.variables.into_iter().map(|variable| view! {
                            <VariableField
                                editor=editor
                                node_id=node_id.clone()
                                variable=variable
                                references=references.clone()
                                can_manage=can_manage
                            />
                        }).collect_view()}
                        <ResolvedPreview editor=editor node_id=preview_node />
                    }
                    .into_any()
                }
            })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn suggestion(
        reference: &str,
        source: ReferenceSource,
        sample: Option<&str>,
    ) -> ReferenceSuggestion {
        ReferenceSuggestion {
            reference: reference.into(),
            label: "libellé".into(),
            source,
            sample: sample.map(str::to_owned),
        }
    }

    #[test]
    fn typed_input_becomes_the_matching_json_value() {
        assert_eq!(
            value_from_input(VariableKind::Number, "23"),
            Some(json!(23))
        );
        assert_eq!(
            value_from_input(VariableKind::Number, "1.5"),
            Some(json!(1.5))
        );
        assert_eq!(
            value_from_input(VariableKind::Boolean, "true"),
            Some(json!(true))
        );
        assert_eq!(
            value_from_input(VariableKind::Boolean, "false"),
            Some(json!(false))
        );
        assert_eq!(
            value_from_input(VariableKind::Text, "fr"),
            Some(json!("fr"))
        );
        assert_eq!(
            value_from_input(VariableKind::Choice, "en"),
            Some(json!("en"))
        );
    }

    #[test]
    fn references_and_unparsable_numbers_stay_text_and_empty_input_removes_the_key() {
        assert_eq!(
            value_from_input(VariableKind::Number, "{{mediaInfo.video.width}}"),
            Some(json!("{{mediaInfo.video.width}}"))
        );
        assert_eq!(
            value_from_input(VariableKind::Number, "beaucoup"),
            Some(json!("beaucoup"))
        );
        assert_eq!(value_from_input(VariableKind::Text, "   "), None);
        assert_eq!(value_from_input(VariableKind::Number, ""), None);
    }

    #[test]
    fn stored_values_are_shown_as_input_text() {
        assert_eq!(input_text(Some(&json!(23))), "23");
        assert_eq!(input_text(Some(&json!("fr"))), "fr");
        assert_eq!(input_text(Some(&json!(true))), "true");
        assert_eq!(input_text(None), "");
    }

    #[test]
    fn a_secret_is_stored_under_secrets_and_an_empty_input_keeps_the_saved_one() {
        let mut config = Map::new();
        apply_secret(&mut config, "token", json!(""));
        assert!(config.is_empty());
        apply_secret(&mut config, "token", json!("t0k"));
        assert_eq!(config[SECRETS_CONFIG_KEY]["token"], json!("t0k"));
        apply_secret(&mut config, "token", json!(""));
        assert_eq!(config[SECRETS_CONFIG_KEY]["token"], json!("t0k"));
        apply_secret(&mut config, "token", Value::Null);
        assert!(config[SECRETS_CONFIG_KEY]["token"].is_null());
    }

    #[test]
    fn a_secret_counts_as_stored_unless_absent_or_cleared() {
        assert!(is_secret_stored(Some(&json!(""))));
        assert!(is_secret_stored(Some(&json!("t0k"))));
        assert!(!is_secret_stored(Some(&Value::Null)));
        assert!(!is_secret_stored(None));
    }

    #[test]
    fn a_picked_reference_is_appended_in_braces() {
        assert_eq!(append_reference("", "workingFile"), "{{workingFile}}");
        assert_eq!(
            append_reference("dossier/", "workDir"),
            "dossier/{{workDir}}"
        );
    }

    #[test]
    fn references_are_grouped_by_source_in_display_order_without_empty_groups() {
        let all = [
            suggestion("mediainfo.video.codec", ReferenceSource::Plugin, None),
            suggestion("workingFile", ReferenceSource::Flow, None),
            suggestion("nodes.a.message", ReferenceSource::Node, None),
            suggestion("workDir", ReferenceSource::Flow, None),
        ];
        let groups = group_references(&all);
        let sources: Vec<ReferenceSource> = groups.iter().map(|(source, _)| *source).collect();
        assert_eq!(
            sources,
            [
                ReferenceSource::Flow,
                ReferenceSource::Node,
                ReferenceSource::Plugin
            ]
        );
        assert_eq!(groups[0].1.len(), 2);
        assert!(group_references(&[]).is_empty());
    }

    #[test]
    fn option_labels_show_the_sample_when_known() {
        assert_eq!(
            reference_option_label(&suggestion(
                "mediaInfo.video.codec",
                ReferenceSource::MediaInfo,
                Some("hevc")
            )),
            "mediaInfo.video.codec — libellé (ex. hevc)"
        );
        assert_eq!(
            reference_option_label(&suggestion("nodes.a.message", ReferenceSource::Node, None)),
            "nodes.a.message — libellé"
        );
    }
}
