//! Éditeur de condition composée : groupes ET/OU imbriqués de règles typées.

use super::condition_edit::{
    ConditionEdit, apply_edit, can_add_group, condition_patch, parse_combinator, read_config,
    working_group,
};
use super::fields::set_fields;
use super::state::Editor;
use forgearr_shared::flows::{
    ConditionCombinator, ConditionGroup, ConditionNode, ConditionOperator, ConditionRule,
};
use leptos::prelude::*;

/// Modification demandée par une ligne : chemin de l'élément et changement.
type EditCallback = Callback<(Vec<usize>, ConditionEdit)>;

fn combinator_label(combinator: ConditionCombinator) -> String {
    match combinator {
        ConditionCombinator::All => format!("Toutes les règles ({})", combinator.label()),
        ConditionCombinator::Any => format!("Au moins une règle ({})", combinator.label()),
    }
}

fn child_path(path: &[usize], index: usize) -> Vec<usize> {
    let mut child = path.to_vec();
    child.push(index);
    child
}

fn rule_view(rule: ConditionRule, path: Vec<usize>, on_edit: EditCallback) -> AnyView {
    let selected = rule.operator;
    let target = rule.target.clone();
    let path = StoredValue::new(path);
    let emit = move |edit: ConditionEdit| on_edit.run((path.get_value(), edit));
    view! {
        <div class="flow-headers__row flow-condition__rule">
            <input class="form-input" placeholder="Propriété (format, vars.codec, any)"
                prop:value=rule.property
                on:change=move |event| emit(ConditionEdit::SetProperty(event_target_value(&event))) />
            <select class="form-select"
                on:change=move |event| emit(ConditionEdit::SetOperator(
                    ConditionOperator::from_loose(&event_target_value(&event)),
                ))>
                {ConditionOperator::ALL.into_iter().map(|operator| view! {
                    <option value={operator.as_str()} selected={operator == selected}>
                        {operator.label()}
                    </option>
                }).collect_view()}
            </select>
            {if selected.needs_target() {
                view! {
                    <input class="form-input" placeholder="Valeur"
                        prop:value=target
                        on:change=move |event| emit(ConditionEdit::SetTarget(event_target_value(&event))) />
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
            <button type="button" class="btn btn-secondary btn-sm"
                on:click=move |_| emit(ConditionEdit::Remove)>"Retirer"</button>
        </div>
    }
    .into_any()
}

fn node_view(node: ConditionNode, path: Vec<usize>, on_edit: EditCallback) -> AnyView {
    match node {
        ConditionNode::Rule(rule) => rule_view(rule, path, on_edit),
        ConditionNode::Group(group) => group_view(group, path, on_edit),
    }
}

fn group_view(group: ConditionGroup, path: Vec<usize>, on_edit: EditCallback) -> AnyView {
    let combinator = group.combinator;
    let negate = group.negate;
    let is_root = path.is_empty();
    let allow_subgroup = can_add_group(&path);
    let children = group
        .rules
        .into_iter()
        .enumerate()
        .map(|(index, child)| node_view(child, child_path(&path, index), on_edit))
        .collect_view();
    let path = StoredValue::new(path);
    let emit = move |edit: ConditionEdit| on_edit.run((path.get_value(), edit));
    view! {
        <div class="flow-condition__group">
            <div class="cluster">
                <select class="form-select"
                    on:change=move |event| emit(ConditionEdit::SetCombinator(
                        parse_combinator(&event_target_value(&event)),
                    ))>
                    {ConditionCombinator::ALL.into_iter().map(|option| view! {
                        <option value={option.as_str()} selected={option == combinator}>
                            {combinator_label(option)}
                        </option>
                    }).collect_view()}
                </select>
                <label class="cluster">
                    <input type="checkbox" prop:checked=negate
                        on:change=move |_| emit(ConditionEdit::ToggleNegate) />
                    <span>"Inverser"</span>
                </label>
                {(!is_root).then(move || view! {
                    <button type="button" class="btn btn-secondary btn-sm"
                        on:click=move |_| emit(ConditionEdit::Remove)>"Retirer le groupe"</button>
                })}
            </div>
            {children}
            <div class="cluster">
                <button type="button" class="btn btn-secondary btn-sm"
                    on:click=move |_| emit(ConditionEdit::AddRule)>"Ajouter une règle"</button>
                <button type="button" class="btn btn-secondary btn-sm" disabled={!allow_subgroup}
                    on:click=move |_| emit(ConditionEdit::AddGroup)>"Ajouter un sous-groupe"</button>
            </div>
        </div>
    }
    .into_any()
}

/// Édite la condition du bloc `node_id` : règle plate ou groupes ET/OU imbriqués.
#[component]
pub fn ConditionEditor(editor: Editor, node_id: String, label: String) -> impl IntoView {
    let id = StoredValue::new(node_id);
    let read = move || {
        editor
            .graph
            .with(|graph| {
                graph
                    .node(&id.read_value())
                    .map(|node| read_config(&node.config))
            })
            .unwrap_or_default()
    };
    let on_edit = Callback::new(move |(path, edit): (Vec<usize>, ConditionEdit)| {
        let mut config = read();
        if apply_edit(&mut config, &path, edit) {
            set_fields(editor, &id.read_value(), condition_patch(&config));
        }
    });
    view! {
        <span class="form-label">{label}</span>
        {move || group_view(working_group(&read()), Vec::new(), on_edit)}
    }
}
