//! Variables de flux d'une notification différée après publication.
//!
//! Le moteur fige `vars.*` sur l'étape différée ; à la publication, le central reconstitue
//! `file.*` (fichier publié), l'identité de la tâche et ces variables, comme le chemin direct.

use super::substitution::{self, RunIdentity};
use forgearr_shared::flows::{FlowVariables, VariableError};
use serde_json::{Map, Value};
use std::path::Path;

/// Variables communes à toutes les notifications d'une tâche : fichier publié et identité.
pub(super) fn base_variables(output: &Path, identity: &RunIdentity<'_>) -> FlowVariables {
    let mut vars = FlowVariables::default();
    substitution::set_file_variables(&mut vars, output);
    substitution::set_identity_variables(&mut vars, identity);
    vars
}

/// Ajoute les variables personnalisées figées sur l'étape différée.
pub(super) fn with_stored_custom(
    base: &FlowVariables,
    stored: &Map<String, Value>,
) -> Result<FlowVariables, VariableError> {
    let mut vars = base.clone();
    vars.merge_custom(stored)?;
    Ok(vars)
}

/// Configuration du bloc HTTP après substitution des variables de flux.
pub(super) fn deferred_config(config: &Value, vars: &FlowVariables) -> Value {
    substitution::substitute_flow_variables_only(config, vars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn identity() -> RunIdentity<'static> {
        RunIdentity {
            job_id: Some("job-1"),
            library_name: Some("Films"),
            worker_name: Some("gpu-1"),
            ..RunIdentity::default()
        }
    }

    #[test]
    fn base_variables_cover_published_file_and_identity() {
        let vars = base_variables(Path::new("/media/films/Film.mkv"), &identity());
        assert_eq!(vars.lookup("file.name"), Some(&json!("Film.mkv")));
        assert_eq!(vars.lookup("library.name"), Some(&json!("Films")));
        assert_eq!(vars.lookup("worker.name"), Some(&json!("gpu-1")));
    }

    #[test]
    fn stored_custom_variables_are_restored() -> Result<(), VariableError> {
        let base = base_variables(Path::new("/media/Film.mkv"), &identity());
        let mut stored = Map::new();
        stored.insert("id".to_owned(), json!("42"));
        let vars = with_stored_custom(&base, &stored)?;
        assert_eq!(vars.lookup("vars.id"), Some(&json!("42")));
        assert!(base.custom.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_stored_variables_are_rejected() {
        let mut stored = Map::new();
        stored.insert("nom invalide".to_owned(), json!(1));
        assert!(with_stored_custom(&FlowVariables::default(), &stored).is_err());
    }

    #[test]
    fn deferred_config_substitutes_file_and_custom_variables() -> Result<(), VariableError> {
        let base = base_variables(Path::new("/media/Film.mkv"), &identity());
        let mut stored = Map::new();
        stored.insert("id".to_owned(), json!("42"));
        let vars = with_stored_custom(&base, &stored)?;
        let config = json!({ "url": "/api/{$vars.id}/{$file.name}" });
        assert_eq!(
            deferred_config(&config, &vars),
            json!({ "url": "/api/42/Film.mkv" })
        );
        Ok(())
    }
}
