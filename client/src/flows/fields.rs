//! Contrôles de réglage générés depuis la fiche du bloc (`FieldSpec`).

use super::state::Editor;
use flux_shared::flows::{FieldKind, FieldSpec};
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
pub fn select_options(
    options: &[(&'static str, &'static str)],
    stored: &str,
) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = options
        .iter()
        .map(|(value, label)| ((*value).to_owned(), (*label).to_owned()))
        .collect();
    if !stored.is_empty() && !options.iter().any(|(value, _)| *value == stored) {
        list.push((stored.to_owned(), format!("{stored} (valeur enregistrée)")));
    }
    list
}

#[component]
fn HeadersField(editor: Editor, node_id: String, field: FieldSpec) -> impl IntoView {
    let rows = move |node_id: &str| -> Vec<(String, String)> {
        current(editor, node_id, field.name)
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
        set_field(editor, node_id, field.name, Value::Object(map));
    };
    let new_name = RwSignal::new(String::new());
    let id = StoredValue::new(node_id);
    view! {
        <div class="form-group">
            <span class="form-label">{field.label}</span>
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
            <p class="form-help">{field.help}</p>
        </div>
    }
}

#[component]
pub fn FieldControl(editor: Editor, node_id: String, field: FieldSpec) -> impl IntoView {
    let input_id = format!("field-{}-{}", node_id, field.name);
    let label = if field.required {
        format!("{} *", field.label)
    } else {
        field.label.to_owned()
    };
    let id = StoredValue::new(node_id.clone());
    let text_value = move || {
        current(editor, &id.read_value(), field.name)
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let control = match field.kind {
        FieldKind::Headers => return view! { <HeadersField editor=editor node_id=node_id field=field /> }.into_any(),
        FieldKind::Bool => view! {
            <label class="cluster">
                <input id=input_id.clone() type="checkbox"
                    prop:checked=move || current(editor, &id.read_value(), field.name).as_bool().unwrap_or(false)
                    on:change=move |event| set_field(editor, &id.read_value(), field.name, Value::Bool(event_target_checked(&event))) />
                <span>{label.clone()}</span>
            </label>
        }.into_any(),
        FieldKind::Select(options) => view! {
            <label for=input_id.clone() class="form-label">{label.clone()}</label>
            <select id=input_id.clone() class="form-select"
                prop:value=text_value
                on:change=move |event| set_field(editor, &id.read_value(), field.name, Value::String(event_target_value(&event)))>
                {move || select_options(options, &text_value()).into_iter().map(|(value, label)| view! {
                    <option value=value>{label}</option>
                }).collect_view()}
            </select>
        }.into_any(),
        FieldKind::Text => view! {
            <label for=input_id.clone() class="form-label">{label.clone()}</label>
            <input id=input_id.clone() class="form-input"
                prop:value=text_value
                on:input=move |event| set_field(editor, &id.read_value(), field.name, Value::String(event_target_value(&event))) />
        }.into_any(),
    };
    view! {
        <div class="form-group">
            {control}
            {(!field.help.is_empty()).then(|| view! { <p class="form-help">{field.help}</p> })}
        </div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_stored_value_stays_selectable() {
        let options = [("eq", "Contient"), ("neq", "Ne contient pas")];
        assert_eq!(select_options(&options, "eq").len(), 2);
        let with_legacy = select_options(&options, "contains");
        assert_eq!(with_legacy.len(), 3);
        assert_eq!(with_legacy[2].0, "contains");
        assert_eq!(select_options(&options, "").len(), 2);
    }
}
