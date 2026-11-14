//! Contrôles de réglage générés depuis la fiche du bloc (`OwnedFieldSpec`).

use super::state::Editor;
use flux_shared::flows::{OwnedFieldKind, OwnedFieldSpec};
use leptos::prelude::*;
use serde_json::{Map, Value};

/// Écrit `value` sous `config.<field>` du bloc `node_id`.
fn set_field(editor: Editor, node_id: &str, field: &str, value: Value) {
    let node_id = node_id.to_owned();
    let field = field.to_owned();
    editor.edit(move |graph| {
        if let Some(node) = graph.node_mut(&node_id) {
            if !node.config.is_object() {
                node.config = Value::Object(Map::new());
            }
            if let Some(config) = node.config.as_object_mut() {
                config.insert(field, value);
            }
        }
    });
}

fn current(editor: Editor, node_id: &str, field: &str) -> Value {
    editor
        .graph
        .with(|graph| {
            graph
                .node(node_id)
                .map(|node| node.config.get(field).cloned())
        })
        .flatten()
        .unwrap_or(Value::Null)
}

/// Options d'une liste, plus la valeur enregistrée si elle n'en fait pas partie.
pub fn select_options(options: &[(String, String)], stored: &str) -> Vec<(String, String)> {
    let mut list = options.to_vec();
    if !stored.is_empty() && !options.iter().any(|(value, _)| value == stored) {
        list.push((stored.to_owned(), format!("{stored} (valeur enregistrée)")));
    }
    list
}

#[component]
fn HeadersField(editor: Editor, node_id: String, field: OwnedFieldSpec) -> impl IntoView {
    let field_name = StoredValue::new(field.name.clone());
    let rows = move |node_id: &str| -> Vec<(String, String)> {
        current(editor, node_id, &field_name.get_value())
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(name, value)| {
                        (name.clone(), value.as_str().unwrap_or_default().to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let write = move |node_id: &str, rows: Vec<(String, String)>| {
        let map: Map<String, Value> = rows
            .into_iter()
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, value)| (name, Value::String(value)))
            .collect();
        set_field(editor, node_id, &field_name.get_value(), Value::Object(map));
    };
    let new_name = RwSignal::new(String::new());
    let id = StoredValue::new(node_id);
    let field_label = field.label.clone();
    let field_help = field.help.clone();
    view! {
        <div class="form-group">
            <span class="form-label">{field_label}</span>
            {move || rows(&id.read_value()).into_iter().enumerate().map(|(index, (name, value))| view! {
                <div class="flow-headers__row">
                    <span class="text-mono">{name}</span>
                    <input class="form-input" type="password" autocomplete="off"
                        placeholder="Valeur enregistrée conservée"
                        prop:value=value
                        on:change=move |event| {
                            let mut list = rows(&id.read_value());
                            if let Some(row) = list.get_mut(index) {
                                row.1 = event_target_value(&event);
                            }
                            write(&id.read_value(), list);
                        } />
                    <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| {
                        let mut list = rows(&id.read_value());
                        if index < list.len() {
                            list.remove(index);
                        }
                        write(&id.read_value(), list);
                    }>"Retirer"</button>
                </div>
            }).collect_view()}
            <div class="flow-headers__row">
                <input class="form-input" placeholder="Nom de l'en-tête"
                    prop:value=move || new_name.get()
                    on:input=move |event| new_name.set(event_target_value(&event)) />
                <span></span>
                <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| {
                    let name = new_name.get_untracked().trim().to_owned();
                    if name.is_empty() {
                        return;
                    }
                    let mut list = rows(&id.read_value());
                    if !list.iter().any(|(existing, _)| *existing == name) {
                        list.push((name, String::new()));
                    }
                    write(&id.read_value(), list);
                    new_name.set(String::new());
                }>"Ajouter"</button>
            </div>
            <p class="form-help">{field_help}</p>
        </div>
    }
}

#[component]
pub fn FieldControl(editor: Editor, node_id: String, field: OwnedFieldSpec) -> impl IntoView {
    let input_id = format!("field-{}-{}", node_id, field.name);
    let label = if field.required {
        format!("{} *", field.label)
    } else {
        field.label.clone()
    };
    let id = StoredValue::new(node_id.clone());
    let field_name = StoredValue::new(field.name.clone());
    let field_help = field.help.clone();
    let control = match field.kind.clone() {
        OwnedFieldKind::Headers => {
            return view! { <HeadersField editor=editor node_id=node_id field=field /> }.into_any();
        }
        OwnedFieldKind::Bool => view! {
            <label class="cluster">
                <input id=input_id.clone() type="checkbox"
                    prop:checked=move || current(editor, &id.read_value(), &field_name.get_value()).as_bool().unwrap_or(false)
                    on:change=move |event| set_field(editor, &id.read_value(), &field_name.get_value(), Value::Bool(event_target_checked(&event))) />
                <span>{label.clone()}</span>
            </label>
        }.into_any(),
        OwnedFieldKind::Select(options) => {
            let options = StoredValue::new(options);
            view! {
                <label for=input_id.clone() class="form-label">{label.clone()}</label>
                <select id=input_id.clone() class="form-select"
                    prop:value=move || current(editor, &id.read_value(), &field_name.get_value()).as_str().unwrap_or_default().to_owned()
                    on:change=move |event| set_field(editor, &id.read_value(), &field_name.get_value(), Value::String(event_target_value(&event)))>
                    {move || {
                        let stored = current(editor, &id.read_value(), &field_name.get_value())
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        select_options(&options.get_value(), &stored).into_iter().map(|(value, label)| view! {
                            <option value=value>{label}</option>
                        }).collect_view()
                    }}
                </select>
            }.into_any()
        }
        OwnedFieldKind::Text => view! {
            <label for=input_id.clone() class="form-label">{label.clone()}</label>
            <input id=input_id.clone() class="form-input"
                prop:value=move || current(editor, &id.read_value(), &field_name.get_value()).as_str().unwrap_or_default().to_owned()
                on:input=move |event| set_field(editor, &id.read_value(), &field_name.get_value(), Value::String(event_target_value(&event))) />
        }.into_any(),
    };
    view! {
        <div class="form-group">
            {control}
            {(!field_help.is_empty()).then(move || view! { <p class="form-help">{field_help.clone()}</p> })}
        </div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_stored_value_stays_selectable() {
        let options = vec![
            ("eq".into(), "Contient".into()),
            ("neq".into(), "Ne contient pas".into()),
        ];
        assert_eq!(select_options(&options, "eq").len(), 2);
        let with_legacy = select_options(&options, "contains");
        assert_eq!(with_legacy.len(), 3);
        assert_eq!(with_legacy[2].0, "contains");
        assert_eq!(select_options(&options, "").len(), 2);
    }
}
