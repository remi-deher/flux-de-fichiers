//! Variables qu'un plugin expose aux scripts, dans sa fiche : référence `{{espace.nom}}`, type et chemin lu (ADR 0025).

use super::metadata::{StatusBanner, StoreTone};
use crate::ui::{Tag, TagTone};
use forgearr_shared::exposed_variables::{ExposedNamespaceView, ExposedVariable, ExposedVariables};
use leptos::prelude::*;

/// Référence à écrire dans la valeur d'une variable de script.
pub fn reference_text(exposed: &ExposedVariables, variable: &ExposedVariable) -> String {
    format!("{{{{{}}}}}", exposed.reference(variable))
}

/// Intitulé d'une variable : son libellé, sinon son nom.
fn variable_title(variable: &ExposedVariable) -> &str {
    if variable.label.trim().is_empty() {
        &variable.name
    } else {
        &variable.label
    }
}

/// Avertissement quand l'espace de noms est partagé avec d'autres plugins.
pub fn conflict_notice(view: &ExposedNamespaceView) -> Option<(String, String)> {
    if view.conflicts_with.is_empty() {
        return None;
    }
    let others = view.conflicts_with.join(", ");
    let namespace = &view.exposes.namespace;
    Some(if view.active {
        (
            format!("Espace de noms « {namespace} » partagé"),
            format!(
                "Aussi déclaré par : {others}. Un script lit l'exposition que désigne sa dépendance (`requires`) ; sans dépendance, c'est celle-ci."
            ),
        )
    } else {
        (
            format!("Espace de noms « {namespace} » déjà pris"),
            format!(
                "Sans dépendance explicite, les scripts lisent l'exposition de : {others}. Ces variables ne sont lues que par un script qui les désigne dans `requires`."
            ),
        )
    })
}

#[component]
fn NamespaceBlock(view: ExposedNamespaceView) -> impl IntoView {
    let notice = conflict_notice(&view);
    let tone = if view.active {
        StoreTone::Warning
    } else {
        StoreTone::Danger
    };
    let contract = format!("contrat {}", view.exposes.contract_version);
    let namespace = view.exposes.namespace.clone();
    let exposed = view.exposes.clone();
    view! {
        <div class="pack-components">
            <div class="step-item__head">
                <Tag label=namespace tone=TagTone::Mono />
                <Tag label=contract tone=TagTone::Neutral />
            </div>
            {notice.map(|(title, text)| view! {
                <StatusBanner title=title text=Some(text) tone=tone />
            })}
            {if exposed.variables.is_empty() {
                view! { <p class="text-dim text-sm">"Aucune variable déclarée."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {exposed.variables.iter().map(|variable| view! {
                            <li class="step-item">
                                <div class="step-item__head">
                                    <span class="text-strong">{variable_title(variable).to_owned()}</span>
                                    <code class="code-snippet">{reference_text(&exposed, variable)}</code>
                                    <Tag label=variable.kind.label() tone=TagTone::Primary />
                                </div>
                                {variable.description.clone().map(|text| view! {
                                    <div class="step-item__meta text-dim">{text}</div>
                                })}
                                <div class="step-item__meta">
                                    "Lu dans " <code class="code-snippet">{variable.path.clone()}</code>
                                </div>
                            </li>
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

/// Section « Variables exposées » : une zone par espace de noms déclaré par le plugin.
#[component]
pub fn ExposedVariablesSection(exposures: Vec<ExposedNamespaceView>) -> impl IntoView {
    if exposures.is_empty() {
        return ().into_any();
    }
    view! {
        <h3 class="form-label">"Variables exposées aux scripts"</h3>
        {exposures.into_iter().map(|view| view! { <NamespaceBlock view=view /> }).collect_view()}
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exposed() -> ExposedVariables {
        ExposedVariables {
            namespace: "mediainfo".into(),
            contract_version: "1.0.0".into(),
            variables: vec![ExposedVariable {
                name: "video.codec".into(),
                label: "Codec vidéo".into(),
                path: "mediainfo.media.track[@type=Video].Format".into(),
                ..ExposedVariable::default()
            }],
        }
    }

    fn view(active: bool, conflicts_with: &[&str]) -> ExposedNamespaceView {
        ExposedNamespaceView {
            provider: "alpha".into(),
            exposes: exposed(),
            active,
            conflicts_with: conflicts_with.iter().map(|id| (*id).to_owned()).collect(),
        }
    }

    #[test]
    fn the_reference_is_written_with_its_braces() {
        let exposure = exposed();
        assert_eq!(
            reference_text(&exposure, &exposure.variables[0]),
            "{{mediainfo.video.codec}}"
        );
    }

    #[test]
    fn a_variable_is_titled_by_its_label_or_else_its_name() {
        let mut variable = exposed().variables[0].clone();
        assert_eq!(variable_title(&variable), "Codec vidéo");
        variable.label = "  ".into();
        assert_eq!(variable_title(&variable), "video.codec");
    }

    #[test]
    fn a_shared_namespace_warns_differently_for_the_active_and_the_shadowed_plugin() {
        assert_eq!(conflict_notice(&view(true, &[])), None);
        let active = conflict_notice(&view(true, &["beta"]));
        assert!(active.is_some_and(|(title, text)| title.contains("partagé")
            && text.contains("beta")
            && text.contains("`requires`")));
        let shadowed = conflict_notice(&view(false, &["beta", "gamma"]));
        assert!(
            shadowed.is_some_and(|(title, text)| title.contains("déjà pris")
                && text.contains("beta, gamma")
                && text.contains("`requires`"))
        );
    }
}
