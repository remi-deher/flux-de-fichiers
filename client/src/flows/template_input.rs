//! Saisie de texte acceptant les modèles `{$variable}` et aide sur le protocole des scripts.

use super::fields::{current, set_field};
use super::state::Editor;
use forgearr_shared::flows::node_references::{
    ReferenceSource, ReferenceSuggestion, flow_variable_suggestions,
};
use forgearr_shared::flows::template_scope::template_token;
use forgearr_shared::flows::{
    ProtocolField, SCRIPT_INPUT_FIELDS, SCRIPT_OUTPUT_FIELDS, SCRIPT_SCHEMA_VERSION,
    is_known_expression, template_tokens,
};
use leptos::prelude::*;
use serde_json::Value;

/// Aide de syntaxe affichée sous chaque champ texte.
const TEMPLATE_HELP: &str = "Modèles : {$file.stem}, {$file.path}, {$vars.nom|défaut}, \
     {$media.video.codec}, {$nodes.<id>.message} ; {{$ pour un « {$ » littéral.";

/// Ordre d'affichage des groupes du sélecteur.
const SOURCE_ORDER: [ReferenceSource; 5] = [
    ReferenceSource::Variables,
    ReferenceSource::Flow,
    ReferenceSource::MediaInfo,
    ReferenceSource::Node,
    ReferenceSource::Plugin,
];

/// Ajoute le jeton `{$expr}` au texte courant d'un champ.
pub fn append_token(current: &str, expr: &str) -> String {
    format!("{current}{}", template_token(expr))
}

/// Expressions regroupées par source, dans l'ordre d'affichage ; les groupes vides sont omis.
pub fn group_references(
    references: &[ReferenceSuggestion],
) -> Vec<(ReferenceSource, Vec<&ReferenceSuggestion>)> {
    SOURCE_ORDER
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

/// Libellé d'une option du sélecteur : l'expression, son intitulé et sa valeur d'exemple.
pub fn reference_option_label(suggestion: &ReferenceSuggestion) -> String {
    match &suggestion.sample {
        Some(sample) => format!(
            "{} — {} (ex. {sample})",
            suggestion.reference, suggestion.label
        ),
        None => format!("{} — {}", suggestion.reference, suggestion.label),
    }
}

/// Expressions d'un modèle que ni le moteur ni les suggestions disponibles ne connaissent.
pub fn unknown_tokens(text: &str, suggestions: &[ReferenceSuggestion]) -> Vec<String> {
    template_tokens(text)
        .into_iter()
        .filter(|expr| {
            !is_known_expression(expr) && !suggestions.iter().any(|known| known.reference == *expr)
        })
        .collect()
}

/// Sélecteur unique d'expressions `{$…}`, groupées par source ; rend l'expression choisie.
#[component]
pub fn TemplatePicker(
    #[prop(into)] suggestions: Signal<Vec<ReferenceSuggestion>>,
    on_pick: Callback<String>,
    #[prop(optional)] disabled: bool,
) -> impl IntoView {
    view! {
        <select class="form-input" disabled=disabled aria-label="Insérer une variable"
            prop:value=""
            on:change=move |event| {
                let expr = event_target_value(&event);
                if !expr.is_empty() {
                    on_pick.run(expr);
                }
            }>
            <option value="">"Insérer une variable…"</option>
            {move || {
                let suggestions = suggestions.get();
                group_references(&suggestions).into_iter().map(|(source, group)| view! {
                    <optgroup label=source.label()>
                        {group.into_iter().map(|suggestion| view! {
                            <option value=suggestion.reference.clone()>{reference_option_label(suggestion)}</option>
                        }).collect_view()}
                    </optgroup>
                }).collect_view()
            }}
        </select>
    }
}

/// Champ texte d'un bloc : saisie libre, insertion de variables et signalement des inconnues.
#[component]
pub fn TemplateInput(
    editor: Editor,
    node_id: StoredValue<String>,
    field_name: StoredValue<String>,
    input_id: String,
    label: String,
) -> impl IntoView {
    let value = move || {
        current(editor, &node_id.read_value(), &field_name.get_value())
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let write = move |text: String| {
        set_field(
            editor,
            &node_id.read_value(),
            &field_name.get_value(),
            Value::String(text),
        );
    };
    let suggestions = Memo::new(move |_| editor.graph.with(flow_variable_suggestions));
    let unknown = move || suggestions.with(|known| unknown_tokens(&value(), known));
    view! {
        <label for=input_id.clone() class="form-label">{label}</label>
        <input id=input_id class="form-input"
            prop:value=value
            on:input=move |event| write(event_target_value(&event)) />
        <p class="form-help">{TEMPLATE_HELP}</p>
        {move || {
            let unknown = unknown();
            (!unknown.is_empty()).then(|| view! {
                <p class="form-help text-warning">
                    {format!("Variables inconnues : {}", unknown.join(", "))}
                </p>
            })
        }}
        <TemplatePicker suggestions=suggestions
            on_pick=Callback::new(move |expr: String| write(append_token(&value(), &expr))) />
    }
}

fn protocol_rows(fields: &'static [ProtocolField]) -> impl IntoView {
    fields
        .iter()
        .map(|(name, description)| {
            view! { <li><code>{*name}</code>" : "{*description}</li> }
        })
        .collect_view()
}

/// Rappel du protocole stdin/stdout des scripts, généré depuis les constantes partagées.
#[component]
pub fn ScriptProtocolHelp() -> impl IntoView {
    view! {
        <details>
            <summary class="form-help">{format!("Protocole (version {SCRIPT_SCHEMA_VERSION})")}</summary>
            <p class="form-help">"Entrée JSON sur stdin :"</p>
            <ul class="form-help">{protocol_rows(&SCRIPT_INPUT_FIELDS)}</ul>
            <p class="form-help">"Réponse JSON facultative sur stdout :"</p>
            <ul class="form-help">{protocol_rows(&SCRIPT_OUTPUT_FIELDS)}</ul>
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::flows::{FlowGraph, IntrinsicVariable};
    use serde_json::json;

    fn graph_with_set_variable(name: &str) -> Result<FlowGraph, serde_json::Error> {
        serde_json::from_value(json!({
            "nodes": [
                {"id": "a", "type": "essentiel.set-variable", "name": "Codec", "config": {"name": name}},
                {"id": "b", "type": "file-output", "name": "Sortie", "config": {"name": "ignoré"}}
            ],
            "connections": []
        }))
    }

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
    fn suggests_intrinsic_and_defined_variables() -> Result<(), serde_json::Error> {
        let suggestions = flow_variable_suggestions(&graph_with_set_variable("codec")?);
        let exprs: Vec<&str> = suggestions.iter().map(|s| s.reference.as_str()).collect();
        assert!(exprs.contains(&"file.stem"));
        assert!(exprs.contains(&"file.path"));
        assert!(exprs.contains(&"vars.codec"));
        assert!(!exprs.contains(&"vars.ignoré"));
        assert_eq!(exprs.len(), IntrinsicVariable::ALL.len() + 1);
        Ok(())
    }

    #[test]
    fn a_picked_expression_is_appended_as_a_token() {
        assert_eq!(append_token("", "file.path"), "{$file.path}");
        assert_eq!(append_token("dossier/", "work.dir"), "dossier/{$work.dir}");
    }

    #[test]
    fn flags_unknown_tokens_only() {
        let plugin = [suggestion(
            "mediainfo.video.codec",
            ReferenceSource::Plugin,
            None,
        )];
        assert!(
            unknown_tokens(
                "{$file.stem}-{$vars.x}{$setting.url}{$media.video.codec}{$nodes.a.message}{{$brut}",
                &[]
            )
            .is_empty()
        );
        assert!(unknown_tokens("{$mediainfo.video.codec}", &plugin).is_empty());
        assert_eq!(
            unknown_tokens("{$fichier.nom}", &plugin),
            vec!["fichier.nom".to_owned()]
        );
    }

    #[test]
    fn references_are_grouped_by_source_in_display_order_without_empty_groups() {
        let all = [
            suggestion("mediainfo.video.codec", ReferenceSource::Plugin, None),
            suggestion("file.path", ReferenceSource::Flow, None),
            suggestion("nodes.a.message", ReferenceSource::Node, None),
            suggestion("vars.x", ReferenceSource::Variables, None),
            suggestion("work.dir", ReferenceSource::Flow, None),
        ];
        let groups = group_references(&all);
        let sources: Vec<ReferenceSource> = groups.iter().map(|(source, _)| *source).collect();
        assert_eq!(
            sources,
            [
                ReferenceSource::Variables,
                ReferenceSource::Flow,
                ReferenceSource::Node,
                ReferenceSource::Plugin
            ]
        );
        assert_eq!(groups.get(1).map(|(_, group)| group.len()), Some(2));
        assert!(group_references(&[]).is_empty());
    }

    #[test]
    fn option_labels_show_the_sample_when_known() {
        assert_eq!(
            reference_option_label(&suggestion(
                "media.video.codec",
                ReferenceSource::MediaInfo,
                Some("hevc")
            )),
            "media.video.codec — libellé (ex. hevc)"
        );
        assert_eq!(
            reference_option_label(&suggestion("nodes.a.message", ReferenceSource::Node, None)),
            "nodes.a.message — libellé"
        );
    }
}
