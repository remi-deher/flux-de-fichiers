//! Essai d'un script sur un worker (ADR 0031) : la réponse JSON qu'un script écrit sur `stdout`, la demande d'essai
//! et son résultat.

use crate::local::ScriptInterpreter;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

/// Durée maximale d'un essai, en secondes.
pub const TRIAL_TIMEOUT_SECONDS: u64 = 30;
/// Octets conservés de `stdout` et de `stderr` dans le résultat d'un essai.
pub const TRIAL_CAPTURE_BYTES: usize = 64 * 1024;

/// Statut retourné par un script sur `stdout` JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptOutcomeStatus {
    #[default]
    Ok,
    Skip,
    Error,
}

/// Réponse JSON optionnelle lue sur `stdout` après exécution du script.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOutput {
    #[serde(default)]
    pub status: ScriptOutcomeStatus,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub next_port: Option<String>,
    #[serde(default)]
    pub working_file: Option<String>,
    #[serde(default)]
    pub media_info: Option<Value>,
    #[serde(default)]
    pub stop_pipeline: bool,
    /// Variables personnalisées à fusionner dans `vars.*` (objet plat nom → scalaire).
    #[serde(default)]
    pub variables: Option<Map<String, Value>>,
}

/// Demande d'essai : le code en cours d'édition, exécuté sur `worker_id`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptTrialRequest {
    pub worker_id: Uuid,
    pub interpreter: ScriptInterpreter,
    pub code: String,
    #[serde(default)]
    pub entrypoint: Option<String>,
    /// Exemple de `config` transmis au script.
    #[serde(default)]
    pub config: Map<String, Value>,
    /// Exemple de `vars.*` transmis au script.
    #[serde(default)]
    pub vars: Map<String, Value>,
}

/// Ce qu'un essai a observé.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptTrialResult {
    /// Entrée JSON envoyée au script.
    pub stdin: String,
    pub stdout: String,
    pub stderr: String,
    /// Absent si le script n'a pas pu démarrer ou a dépassé le délai.
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
    /// `stdout` ou `stderr` a été coupé à `TRIAL_CAPTURE_BYTES`.
    pub truncated: bool,
    /// `stdout` lu comme réponse du script ; absent si `stdout` est vide ou invalide.
    pub response: Option<ScriptOutput>,
    /// Pourquoi `stdout` n'est pas une réponse valide.
    pub response_error: Option<String>,
    /// Pourquoi le script n'a pas pu démarrer (interpréteur absent, dossier de travail…).
    pub launch_error: Option<String>,
}

/// Lit `stdout` comme réponse d'un script : `Ok(None)` s'il est vide.
pub fn parse_script_response(stdout: &str) -> Result<Option<ScriptOutput>, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(trimmed)
        .map(Some)
        .map_err(|err| format!("sortie JSON du script invalide : {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_stdout_is_no_response_and_garbage_is_an_error() {
        assert_eq!(parse_script_response("  \n"), Ok(None));
        assert!(parse_script_response("pas du json").is_err());
    }

    #[test]
    fn a_response_defaults_to_ok_and_reads_camel_case_fields() -> Result<(), String> {
        let response = parse_script_response(r#"{"message":"fait","nextPort":"suite"}"#)?
            .ok_or("réponse absente")?;
        assert_eq!(response.status, ScriptOutcomeStatus::Ok);
        assert_eq!(response.message.as_deref(), Some("fait"));
        assert_eq!(response.next_port.as_deref(), Some("suite"));
        Ok(())
    }
}
