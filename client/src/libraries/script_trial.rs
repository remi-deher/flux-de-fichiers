//! Essai d'un script sur un worker (ADR 0031, lot 3c) : état partagé par le bouton « Tester » et le panneau
//! « Essai », et les fonctions pures qui préparent la demande et résument le résultat.

use crate::api::{self, ApiError};
use crate::ui::TagTone;
use chrono::{DateTime, Utc};
use forgearr_shared::local::ScriptInterpreter;
use forgearr_shared::script_trial::{
    ScriptOutcomeStatus, ScriptOutput, ScriptTrialRequest, ScriptTrialResult,
};
use forgearr_shared::script_variables::{ScriptVariable, VariableKind};
use forgearr_shared::workers::Worker;
use leptos::prelude::*;
use serde_json::{Map, Value};
use uuid::Uuid;

const TRIAL_PATH: &str = "/api/v1/local/scripts/trial";
/// Code d'erreur du central quand le worker choisi est hors ligne ou muet (ADR 0031).
const WORKER_UNREACHABLE_CODE: &str = "central_unavailable";
/// Texte d'un objet JSON vide, valeur initiale de `config`.
const EMPTY_OBJECT_TEXT: &str = "{}";

/// Un worker proposé pour l'essai.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkerChoice {
    pub(super) id: Uuid,
    pub(super) label: String,
    /// Le central peut lui confier un essai maintenant (intégré ou distant en ligne).
    pub(super) available: bool,
}

/// Workers proposés : l'intégré d'abord, puis les distants en ligne, puis les hors ligne (non choisissables).
pub(super) fn worker_choices(workers: &[Worker], now: DateTime<Utc>) -> Vec<WorkerChoice> {
    let mut choices: Vec<(u8, WorkerChoice)> = workers
        .iter()
        .map(|worker| {
            let (rank, suffix, available) = if worker.is_integrated() {
                (0, " (intégré au central)", true)
            } else if worker.is_online(now) {
                (1, "", true)
            } else {
                (2, " (hors ligne)", false)
            };
            let choice = WorkerChoice {
                id: worker.id,
                label: format!("{}{suffix}", worker.name),
                available,
            };
            (rank, choice)
        })
        .collect();
    choices
        .sort_by(|(a_rank, a), (b_rank, b)| a_rank.cmp(b_rank).then_with(|| a.label.cmp(&b.label)));
    choices.into_iter().map(|(_, choice)| choice).collect()
}

/// Premier worker choisissable, proposé par défaut.
pub(super) fn default_worker(choices: &[WorkerChoice]) -> Option<Uuid> {
    choices
        .iter()
        .find(|choice| choice.available)
        .map(|choice| choice.id)
}

/// Exemple de `vars.*` : la valeur par défaut de chaque variable déclarée, sauf les secrets (ADR 0031, point 5).
pub(super) fn sample_vars(variables: &[ScriptVariable]) -> Map<String, Value> {
    variables
        .iter()
        .filter(|variable| variable.kind != VariableKind::Secret)
        .filter_map(|variable| {
            variable
                .default_value
                .clone()
                .map(|value| (variable.name.clone(), value))
        })
        .collect()
}

/// Objet JSON en texte indenté, pour pré-remplir une zone de saisie.
pub(super) fn object_text(object: &Map<String, Value>) -> String {
    if object.is_empty() {
        return EMPTY_OBJECT_TEXT.to_owned();
    }
    serde_json::to_string_pretty(object).unwrap_or_else(|_| EMPTY_OBJECT_TEXT.to_owned())
}

/// Lit un objet JSON saisi ; un texte vide vaut un objet vide.
pub(super) fn parse_object(text: &str, field: &str) -> Result<Map<String, Value>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(format!("{field} doit être un objet JSON ({{…}}).")),
        Err(err) => Err(format!("{field} n'est pas un JSON valide : {err}.")),
    }
}

/// Ce que le bouton « Tester » envoie, lu dans l'éditeur au moment du clic.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TrialInput {
    pub(super) interpreter: ScriptInterpreter,
    pub(super) code: String,
    pub(super) entrypoint: String,
}

/// Demande d'essai, ou la raison pour laquelle elle ne peut pas partir.
pub(super) fn build_request(
    worker: Option<Uuid>,
    input: TrialInput,
    config_text: &str,
    vars_text: &str,
) -> Result<ScriptTrialRequest, String> {
    let worker_id = worker.ok_or("Choisissez un worker en ligne pour l'essai.")?;
    if input.code.trim().is_empty() {
        return Err("Le script est vide : rien à essayer.".to_owned());
    }
    let entrypoint = input.entrypoint.trim();
    Ok(ScriptTrialRequest {
        worker_id,
        interpreter: input.interpreter,
        code: input.code,
        entrypoint: (!entrypoint.is_empty()).then(|| entrypoint.to_owned()),
        config: parse_object(config_text, "config")?,
        vars: parse_object(vars_text, "vars")?,
    })
}

/// Message d'un échec de l'appel, précisé pour un worker injoignable.
pub(super) fn failure_message(err: &ApiError) -> String {
    if err.code() == Some(WORKER_UNREACHABLE_CODE) {
        "Le worker choisi est hors ligne ou n'a pas répondu à temps.".to_owned()
    } else {
        err.message()
    }
}

/// Verdict de l'exécution, en une étiquette.
pub(super) fn verdict(result: &ScriptTrialResult) -> (String, TagTone) {
    if result.launch_error.is_some() {
        return ("Lancement impossible".to_owned(), TagTone::Danger);
    }
    if result.timed_out {
        return ("Délai dépassé".to_owned(), TagTone::Danger);
    }
    match result.exit_code {
        Some(0) => ("Code de sortie 0".to_owned(), TagTone::Success),
        Some(code) => (format!("Code de sortie {code}"), TagTone::Danger),
        None => ("Arrêté sans code de sortie".to_owned(), TagTone::Warning),
    }
}

pub(super) const fn status_label(status: ScriptOutcomeStatus) -> (&'static str, TagTone) {
    match status {
        ScriptOutcomeStatus::Ok => ("ok", TagTone::Success),
        ScriptOutcomeStatus::Skip => ("skip", TagTone::Warning),
        ScriptOutcomeStatus::Error => ("error", TagTone::Danger),
    }
}

/// Champs renseignés de la réponse interprétée, hors statut, dans l'ordre du contrat.
pub(super) fn response_fields(output: &ScriptOutput) -> Vec<(&'static str, String)> {
    let json = |value: &Value| serde_json::to_string_pretty(value).unwrap_or_default();
    let mut fields = Vec::new();
    let texts = [
        ("message", &output.message),
        ("nextPort", &output.next_port),
        ("workingFile", &output.working_file),
    ];
    for (name, value) in texts {
        if let Some(text) = value {
            fields.push((name, text.clone()));
        }
    }
    if output.stop_pipeline {
        fields.push(("stopPipeline", "true".to_owned()));
    }
    if let Some(variables) = &output.variables {
        fields.push(("variables", json(&Value::Object(variables.clone()))));
    }
    if let Some(media_info) = &output.media_info {
        fields.push(("mediaInfo", json(media_info)));
    }
    fields
}

/// État d'un essai, partagé par le bouton de la barre et le panneau.
#[derive(Clone, Copy)]
pub(super) struct ScriptTrial {
    pub(super) worker: RwSignal<Option<Uuid>>,
    pub(super) config_text: RwSignal<String>,
    pub(super) vars_text: RwSignal<String>,
    /// Dernier résultat, ou le message d'échec de la demande.
    pub(super) outcome: RwSignal<Option<Result<ScriptTrialResult, String>>>,
    action: Action<ScriptTrialRequest, Result<ScriptTrialResult, ApiError>>,
}

impl ScriptTrial {
    pub(super) fn new(variables: &[ScriptVariable]) -> Self {
        let action = Action::new_local(move |req: &ScriptTrialRequest| {
            let req = req.clone();
            async move {
                api::post::<ScriptTrialRequest, ScriptTrialResult>(TRIAL_PATH, Some(&req)).await
            }
        });
        let outcome = RwSignal::new(None);
        Effect::new(move |_| {
            if let Some(result) = action.value().get() {
                outcome.set(Some(result.map_err(|err| failure_message(&err))));
            }
        });
        Self {
            worker: RwSignal::new(None),
            config_text: RwSignal::new(EMPTY_OBJECT_TEXT.to_owned()),
            vars_text: RwSignal::new(object_text(&sample_vars(variables))),
            outcome,
            action,
        }
    }

    pub(super) fn pending(self) -> Signal<bool> {
        Signal::derive(move || self.action.pending().get())
    }

    /// Lance l'essai, ou affiche pourquoi la demande ne peut pas partir.
    pub(super) fn run(self, input: TrialInput) {
        let request = build_request(
            self.worker.get_untracked(),
            input,
            &self.config_text.get_untracked(),
            &self.vars_text.get_untracked(),
        );
        match request {
            Ok(request) => {
                self.outcome.set(None);
                self.action.dispatch(request);
            }
            Err(message) => self.outcome.set(Some(Err(message))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn worker(id: u128, name: &str, last_seen: DateTime<Utc>) -> Worker {
        serde_json::from_value(json!({
            "id": Uuid::from_u128(id),
            "name": name,
            "last_seen": last_seen,
        }))
        .unwrap_or_else(|err| panic!("worker de test invalide : {err}"))
    }

    fn input(code: &str, entrypoint: &str) -> TrialInput {
        TrialInput {
            interpreter: ScriptInterpreter::Sh,
            code: code.to_owned(),
            entrypoint: entrypoint.to_owned(),
        }
    }

    #[test]
    fn the_integrated_worker_comes_first_and_offline_workers_are_not_available() {
        let now = Utc::now();
        let long_ago = now - chrono::Duration::days(1);
        let integrated = forgearr_shared::workers::INTEGRATED_WORKER_NAME;
        let workers = [
            worker(1, "zeta", now),
            worker(2, "absent", long_ago),
            worker(3, integrated, long_ago),
            worker(4, "alpha", now),
        ];
        let choices = worker_choices(&workers, now);
        let ids: Vec<u128> = choices.iter().map(|choice| choice.id.as_u128()).collect();
        assert_eq!(ids, [3, 4, 1, 2]);
        assert!(choices[0].label.ends_with("(intégré au central)"));
        assert_eq!(choices[3].label, "absent (hors ligne)");
        assert!(!choices[3].available);
        assert_eq!(default_worker(&choices), Some(Uuid::from_u128(3)));
        assert_eq!(default_worker(&choices[3..]), None);
    }

    #[test]
    fn sample_vars_keeps_defaults_but_never_secrets() {
        let variable = |name: &str, kind, default: Option<Value>| ScriptVariable {
            name: name.to_owned(),
            kind,
            default_value: default,
            ..ScriptVariable::default()
        };
        let vars = sample_vars(&[
            variable("langue", VariableKind::Text, Some(json!("fr"))),
            variable("jeton", VariableKind::Secret, Some(json!("s3cr3t"))),
            variable("seuil", VariableKind::Number, None),
        ]);
        assert_eq!(Value::Object(vars.clone()), json!({"langue": "fr"}));
        assert!(object_text(&vars).contains("\"langue\": \"fr\""));
        assert_eq!(object_text(&Map::new()), "{}");
    }

    #[test]
    fn parse_object_accepts_objects_and_blank_text_only() {
        assert_eq!(parse_object("  ", "config"), Ok(Map::new()));
        assert_eq!(
            parse_object(r#"{"a": 1}"#, "config").map(Value::Object),
            Ok(json!({"a": 1}))
        );
        assert!(parse_object("[1]", "vars").is_err_and(|msg| msg.contains("objet JSON")));
        assert!(parse_object("{", "vars").is_err_and(|msg| msg.starts_with("vars")));
    }

    #[test]
    fn build_request_needs_a_worker_and_code_and_trims_the_entrypoint() {
        let id = Uuid::from_u128(9);
        assert!(build_request(None, input("true", ""), "{}", "{}").is_err());
        assert!(build_request(Some(id), input("  ", ""), "{}", "{}").is_err());
        assert!(build_request(Some(id), input("true", ""), "[]", "{}").is_err());
        let request = build_request(Some(id), input("true", " run.sh "), r#"{"x":1}"#, "")
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(request.worker_id, id);
        assert_eq!(request.entrypoint.as_deref(), Some("run.sh"));
        assert_eq!(request.config["x"], 1);
        assert!(request.vars.is_empty());
        let blank_entry = build_request(Some(id), input("true", "  "), "", "")
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(blank_entry.entrypoint, None);
    }

    #[test]
    fn an_unreachable_worker_gets_its_own_message() {
        let unreachable = ApiError::Rejected {
            status: 502,
            code: WORKER_UNREACHABLE_CODE.to_owned(),
            detail: None,
        };
        assert!(failure_message(&unreachable).contains("hors ligne"));
        assert_eq!(
            failure_message(&ApiError::Network),
            ApiError::Network.message()
        );
    }

    #[test]
    fn the_verdict_reports_launch_errors_timeouts_and_exit_codes() {
        let mut result = ScriptTrialResult {
            exit_code: Some(0),
            ..ScriptTrialResult::default()
        };
        assert_eq!(verdict(&result).1, TagTone::Success);
        result.exit_code = Some(2);
        assert_eq!(verdict(&result).0, "Code de sortie 2");
        result.timed_out = true;
        assert_eq!(verdict(&result).0, "Délai dépassé");
        result.launch_error = Some("python3 introuvable".to_owned());
        assert_eq!(verdict(&result).0, "Lancement impossible");
        let silent = ScriptTrialResult::default();
        assert_eq!(verdict(&silent).1, TagTone::Warning);
    }

    #[test]
    fn response_fields_list_only_what_the_script_returned() {
        let output = ScriptOutput {
            message: Some("fait".to_owned()),
            next_port: Some("suite".to_owned()),
            stop_pipeline: true,
            variables: Some(Map::from_iter([("n".to_owned(), json!(2))])),
            ..ScriptOutput::default()
        };
        let names: Vec<&str> = response_fields(&output)
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names, ["message", "nextPort", "stopPipeline", "variables"]);
        assert!(response_fields(&ScriptOutput::default()).is_empty());
        assert_eq!(status_label(ScriptOutcomeStatus::Skip).0, "skip");
    }
}
