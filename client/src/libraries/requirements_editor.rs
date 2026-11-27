//! Déclaration des dépendances d'un script local (`requires`) : lignes éditables, choix d'une variable
//! exposée par un plugin du catalogue ou saisie manuelle (ADR 0025).

use super::rows::{KeyedRow, edit_row, next_key, read_row, remove_row, row_keys};
use crate::api;
use forgearr_shared::exposed_variables::{
    DEFAULT_CONTRACT_MAJOR, ExposedNamespaceView, RepositoryPointer, ScriptRequirement,
    validate_requirements,
};
use leptos::prelude::*;

const EXPOSED_VARIABLES_PATH: &str = "/api/v1/plugins/exposed-variables";
const CHOICE_SEPARATOR: char = '|';
const CHOICE_PARTS: usize = 3;

/// Ligne en cours d'édition : les textes saisis tels quels, convertis à l'enregistrement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequirementRow {
    key: u32,
    variable: String,
    provider: String,
    major_text: String,
    repository_id: String,
    repository_url: String,
}

impl KeyedRow for RequirementRow {
    fn key(&self) -> u32 {
        self.key
    }
}

/// Lignes éditables d'après les dépendances enregistrées.
pub fn requirement_rows_from(requirements: Vec<ScriptRequirement>) -> Vec<RequirementRow> {
    (0u32..)
        .zip(requirements)
        .map(|(key, requirement)| {
            let (repository_id, repository_url) = requirement
                .repository
                .map(|repository| (repository.id, repository.index_url))
                .unwrap_or_default();
            RequirementRow {
                key,
                variable: requirement.variable,
                provider: requirement.provider,
                major_text: requirement.contract_major.to_string(),
                repository_id,
                repository_url,
            }
        })
        .collect()
}

fn row_to_requirement(row: &RequirementRow) -> Result<ScriptRequirement, String> {
    let variable = row.variable.trim().to_owned();
    let major_text = row.major_text.trim();
    let contract_major = if major_text.is_empty() {
        DEFAULT_CONTRACT_MAJOR
    } else {
        major_text.parse::<u32>().map_err(|_| {
            format!("« {variable} » : version majeure invalide (nombre entier attendu).")
        })?
    };
    let (id, url) = (row.repository_id.trim(), row.repository_url.trim());
    let repository = match (id.is_empty(), url.is_empty()) {
        (true, true) => None,
        (false, false) => Some(RepositoryPointer {
            id: id.to_owned(),
            index_url: url.to_owned(),
        }),
        _ => {
            return Err(format!(
                "« {variable} » : le dépôt demande un identifiant et l'URL de son index."
            ));
        }
    };
    Ok(ScriptRequirement {
        variable,
        provider: row.provider.trim().to_owned(),
        contract_major,
        repository,
    })
}

/// Dépendances validées d'après les lignes saisies, ou le message à afficher à l'utilisateur.
pub fn rows_to_requirements(rows: &[RequirementRow]) -> Result<Vec<ScriptRequirement>, String> {
    let requirements = rows
        .iter()
        .map(row_to_requirement)
        .collect::<Result<Vec<_>, _>>()?;
    validate_requirements(&requirements).map_err(|err| err.to_string())?;
    Ok(requirements)
}

/// Variable exposée proposée dans la liste de choix : fournisseur, référence complète et version majeure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExposedChoice {
    pub value: String,
    pub label: String,
}

/// Choix d'après les expositions du catalogue ; une exposition masquée par une autre est signalée.
pub fn exposed_choices(views: &[ExposedNamespaceView]) -> Vec<ExposedChoice> {
    views
        .iter()
        .flat_map(|view| {
            let major = view
                .exposes
                .contract_major()
                .unwrap_or(DEFAULT_CONTRACT_MAJOR);
            view.exposes.variables.iter().map(move |variable| {
                let reference = view.exposes.reference(variable);
                let shadowed = if view.active {
                    ""
                } else {
                    " (espace contesté)"
                };
                ExposedChoice {
                    value: format!(
                        "{}{CHOICE_SEPARATOR}{reference}{CHOICE_SEPARATOR}{major}",
                        view.provider
                    ),
                    label: format!(
                        "{reference} — {} (contrat {major}){shadowed}",
                        view.provider
                    ),
                }
            })
        })
        .collect()
}

/// Fournisseur, référence et version majeure d'un choix.
pub fn parse_choice(value: &str) -> Option<(String, String, u32)> {
    let mut parts = value.splitn(CHOICE_PARTS, CHOICE_SEPARATOR);
    let provider = parts.next().filter(|part| !part.is_empty())?;
    let variable = parts.next().filter(|part| !part.is_empty())?;
    let major = parts.next()?.parse::<u32>().ok()?;
    Some((provider.to_owned(), variable.to_owned(), major))
}

#[component]
fn RequirementRowEditor(
    rows: RwSignal<Vec<RequirementRow>>,
    key: u32,
    choices: Signal<Vec<ExposedChoice>>,
    read_only: bool,
) -> impl IntoView {
    let text = move |read: fn(&RequirementRow) -> String| {
        move || read_row(rows, key, read).unwrap_or_default()
    };
    view! {
        <div class="step-item">
            <div class="form-group">
                <label class="form-label">"Variable exposée du catalogue"</label>
                <select class="form-input" disabled=read_only prop:value=""
                    on:change=move |event| {
                        if let Some((provider, variable, major)) = parse_choice(&event_target_value(&event)) {
                            edit_row(rows, key, |row| {
                                row.provider = provider;
                                row.variable = variable;
                                row.major_text = major.to_string();
                            });
                        }
                    }>
                    <option value="">"Choisir pour remplir les champs…"</option>
                    {move || choices.get().into_iter().map(|choice| view! {
                        <option value=choice.value>{choice.label}</option>
                    }).collect_view()}
                </select>
            </div>
            <div class="form-group">
                <label class="form-label">"Référence (espace.nom)"</label>
                <input class="form-input text-mono" disabled=read_only placeholder="mediainfo.video.codec"
                    prop:value=text(|row| row.variable.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.variable = event_target_value(&e)) />
            </div>
            <div class="form-group">
                <label class="form-label">"Extension fournisseur"</label>
                <input class="form-input text-mono" disabled=read_only placeholder="app.mediainfo"
                    prop:value=text(|row| row.provider.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.provider = event_target_value(&e)) />
            </div>
            <div class="form-group">
                <label class="form-label">"Version majeure du contrat"</label>
                <input class="form-input" disabled=read_only placeholder="1"
                    prop:value=text(|row| row.major_text.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.major_text = event_target_value(&e)) />
            </div>
            <div class="cluster">
                <input class="form-input" disabled=read_only aria-label="Identifiant du dépôt"
                    placeholder="Dépôt (facultatif) : identifiant"
                    prop:value=text(|row| row.repository_id.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.repository_id = event_target_value(&e)) />
                <input class="form-input" disabled=read_only aria-label="URL de l'index du dépôt"
                    placeholder="https://…/index.json"
                    prop:value=text(|row| row.repository_url.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.repository_url = event_target_value(&e)) />
            </div>
            {(!read_only).then(|| view! {
                <button type="button" class="btn btn-danger-quiet btn-sm"
                    on:click=move |_| remove_row(rows, key)>
                    "Retirer la dépendance"
                </button>
            })}
        </div>
    }
}

/// Liste éditable des dépendances d'un script, avec la case « intégrer sans dépendance ».
#[component]
pub fn RequirementsEditor(
    rows: RwSignal<Vec<RequirementRow>>,
    unverified: RwSignal<bool>,
    read_only: bool,
) -> impl IntoView {
    let catalog =
        LocalResource::new(|| api::get::<Vec<ExposedNamespaceView>>(EXPOSED_VARIABLES_PATH));
    let choices = Signal::derive(move || {
        catalog
            .get()
            .and_then(Result::ok)
            .map(|views| exposed_choices(&views))
            .unwrap_or_default()
    });
    let add = move |_| {
        rows.update(|all| {
            let key = next_key(all);
            all.push(RequirementRow {
                key,
                major_text: DEFAULT_CONTRACT_MAJOR.to_string(),
                ..RequirementRow::default()
            });
        });
    };
    view! {
        <div class="form-group">
            <span class="form-label">"Dépendances sur des variables d'extensions"</span>
            <p class="form-help">
                "Variables exposées par une extension que le script utilise : sans le fournisseur, le script est indisponible."
            </p>
            <For
                each=move || row_keys(rows)
                key=|key| *key
                children=move |key| view! {
                    <RequirementRowEditor rows=rows key=key choices=choices read_only=read_only />
                }
            />
            {(!read_only).then(|| view! {
                <button type="button" class="btn btn-secondary btn-sm" on:click=add>"Ajouter une dépendance"</button>
            })}
            <label class="form-label">
                <input type="checkbox" disabled=read_only
                    prop:checked=move || unverified.get()
                    on:change=move |e| unverified.set(event_target_checked(&e)) />
                " Intégrer sans dépendance (non vérifié : variables saisies à la main)"
            </label>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::exposed_variables::{ExposedVariable, ExposedVariables};

    fn row(variable: &str, provider: &str) -> RequirementRow {
        RequirementRow {
            key: 0,
            variable: variable.into(),
            provider: provider.into(),
            major_text: "1".into(),
            ..RequirementRow::default()
        }
    }

    fn requirement(variable: &str, provider: &str) -> ScriptRequirement {
        ScriptRequirement {
            variable: variable.into(),
            provider: provider.into(),
            contract_major: 1,
            repository: None,
        }
    }

    #[test]
    fn stored_requirements_round_trip_through_the_rows() {
        let mut with_repository = requirement("exiftool.camera.model", "app.exiftool");
        with_repository.contract_major = 2;
        with_repository.repository = Some(RepositoryPointer {
            id: "repo.tiers".into(),
            index_url: "https://exemple.org/p/index.json".into(),
        });
        let stored = vec![
            requirement("mediainfo.video.codec", "app.mediainfo"),
            with_repository,
        ];
        assert_eq!(
            rows_to_requirements(&requirement_rows_from(stored.clone())),
            Ok(stored)
        );
    }

    #[test]
    fn an_empty_major_means_the_first_one_and_a_bad_major_names_the_variable() {
        let mut first = row("mediainfo.video.codec", "app.mediainfo");
        first.major_text = "  ".into();
        assert_eq!(
            rows_to_requirements(&[first]).map(|r| r[0].contract_major),
            Ok(DEFAULT_CONTRACT_MAJOR)
        );
        let mut bad = row("mediainfo.video.codec", "app.mediainfo");
        bad.major_text = "deux".into();
        let error = rows_to_requirements(&[bad]).err().unwrap_or_default();
        assert!(error.contains("mediainfo.video.codec") && error.contains("version majeure"));
    }

    #[test]
    fn a_repository_needs_both_its_id_and_its_index_url() {
        let mut half = row("a.b", "app.x");
        half.repository_id = "repo.tiers".into();
        assert!(
            rows_to_requirements(&[half])
                .err()
                .is_some_and(|e| e.contains("dépôt"))
        );
        let mut other_half = row("a.b", "app.x");
        other_half.repository_url = "https://exemple.org/index.json".into();
        assert!(rows_to_requirements(&[other_half]).is_err());
    }

    #[test]
    fn schema_errors_are_reported_to_the_user() {
        assert!(rows_to_requirements(&[row("codec", "app.mediainfo")]).is_err());
        let ambiguous = [
            row("mediainfo.video.codec", "app.a"),
            row("mediainfo.audio.codec", "app.b"),
        ];
        assert!(
            rows_to_requirements(&ambiguous)
                .err()
                .is_some_and(|e| e.contains("deux fournisseurs"))
        );
        assert_eq!(rows_to_requirements(&[]), Ok(Vec::new()));
    }

    fn view(provider: &str, active: bool) -> ExposedNamespaceView {
        ExposedNamespaceView {
            provider: provider.into(),
            exposes: ExposedVariables {
                namespace: "mediainfo".into(),
                contract_version: "2.1.0".into(),
                variables: vec![ExposedVariable {
                    name: "video.codec".into(),
                    path: "p".into(),
                    ..ExposedVariable::default()
                }],
            },
            active,
            conflicts_with: Vec::new(),
        }
    }

    #[test]
    fn catalogue_choices_carry_provider_reference_and_major_and_flag_shadowed_namespaces() {
        let choices = exposed_choices(&[view("alpha", true), view("beta", false)]);
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].value, "alpha|mediainfo.video.codec|2");
        assert!(
            choices[0].label.contains("mediainfo.video.codec")
                && !choices[0].label.contains("contesté")
        );
        assert!(choices[1].label.contains("(espace contesté)"));
        assert!(exposed_choices(&[]).is_empty());
    }

    #[test]
    fn a_choice_value_is_parsed_back_and_malformed_ones_are_ignored() {
        assert_eq!(
            parse_choice("alpha|mediainfo.video.codec|2"),
            Some(("alpha".into(), "mediainfo.video.codec".into(), 2))
        );
        for bad in [
            "",
            "alpha",
            "alpha|a.b",
            "alpha|a.b|x",
            "|a.b|1",
            "alpha||1",
        ] {
            assert_eq!(parse_choice(bad), None, "{bad}");
        }
    }
}
