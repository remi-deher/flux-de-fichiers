//! Dépendances d'un script du catalogue sur les variables exposées par des plugins (ADR 0025).

use crate::flows::OperationRegistry;
use forgearr_shared::{
    exposed_variables::{
        ExposedVariables, RequirementStatus, ScriptRequirement, validate_exposed,
        validate_requirements,
    },
    script_variables::{ScriptVariable, validate_variables},
};
use serde_json::Value;
use std::{fs, path::Path};

/// Fichier de l'opération d'un script, dans le dossier de sa fiche.
const SCRIPT_OPERATION_FILE: &str = "operations/run.json";

/// Dépendances lues dans l'opération d'un script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptRequirements {
    pub requires: Vec<ScriptRequirement>,
    /// Script intégré sans ses dépendances : elles ne sont ni contrôlées ni installées.
    pub dependencies_unverified: bool,
}

/// Lit `runtime.requires` et `runtime.dependenciesUnverified` ; vide si l'opération manque ou est illisible.
pub fn read_script_requirements(item_dir: &Path) -> ScriptRequirements {
    let Some(runtime) = fs::read_to_string(item_dir.join(SCRIPT_OPERATION_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|operation| operation.get("runtime").cloned())
    else {
        return ScriptRequirements::default();
    };
    ScriptRequirements {
        requires: runtime
            .get("requires")
            .and_then(|raw| serde_json::from_value(raw.clone()).ok())
            .unwrap_or_default(),
        dependencies_unverified: runtime
            .get("dependenciesUnverified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// Disponibilité de chaque dépendance dans les plugins chargés par `registry`.
pub fn requirement_statuses(
    requirements: &[ScriptRequirement],
    registry: &OperationRegistry,
) -> Vec<RequirementStatus> {
    requirements
        .iter()
        .map(|requirement| RequirementStatus {
            requirement: requirement.clone(),
            available: registry.is_requirement_met(requirement),
        })
        .collect()
}

fn exposes_issue(raw: Option<&Value>) -> Option<String> {
    let raw = raw?;
    match serde_json::from_value::<ExposedVariables>(raw.clone()) {
        Ok(exposed) => validate_exposed(&exposed).err().map(|err| err.to_string()),
        Err(err) => Some(format!("exposes illisible : {err}")),
    }
}

fn runtime_issue(runtime: &Value) -> Option<String> {
    if let Some(raw) = runtime.get("variables") {
        match serde_json::from_value::<Vec<ScriptVariable>>(raw.clone()) {
            Ok(variables) => {
                if let Err(err) = validate_variables(&variables) {
                    return Some(err.to_string());
                }
            }
            Err(err) => return Some(format!("variables illisibles : {err}")),
        }
    }
    let raw = runtime.get("requires")?;
    match serde_json::from_value::<Vec<ScriptRequirement>>(raw.clone()) {
        Ok(requirements) => validate_requirements(&requirements)
            .err()
            .map(|err| err.to_string()),
        Err(err) => Some(format!("requires illisible : {err}")),
    }
}

/// Première raison pour laquelle un document d'opération déclare mal ses variables, expositions
/// ou dépendances ; `None` s'il est valide ou n'est pas un contrat d'opération.
pub fn operation_declaration_issue(content: &str) -> Option<String> {
    let operation: Value = serde_json::from_str(content).ok()?;
    let runtime = operation.get("runtime")?;
    operation.get("providerId")?;
    exposes_issue(operation.get("exposes")).or_else(|| runtime_issue(runtime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn requirement(variable: &str, provider: &str) -> ScriptRequirement {
        ScriptRequirement {
            variable: variable.into(),
            provider: provider.into(),
            contract_major: 1,
            repository: None,
        }
    }

    #[test]
    fn requirements_are_read_from_the_script_operation() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        fs::create_dir_all(dir.path().join("operations"))?;
        fs::write(
            dir.path().join(SCRIPT_OPERATION_FILE),
            json!({"runtime": {"requires": [{"variable": "mediainfo.video.codec", "provider": "app.mediainfo"}],
                               "dependenciesUnverified": true}})
            .to_string(),
        )?;
        let read = read_script_requirements(dir.path());
        assert_eq!(
            read.requires,
            vec![requirement("mediainfo.video.codec", "app.mediainfo")]
        );
        assert!(read.dependencies_unverified);
        assert_eq!(
            read_script_requirements(&dir.path().join("absent")),
            ScriptRequirements::default()
        );
        Ok(())
    }

    #[test]
    fn statuses_follow_the_providers_loaded_in_the_registry() -> anyhow::Result<()> {
        let mut registry = OperationRegistry::new();
        registry.register_json(json!({
            "schemaVersion": "1.1.0", "id": "operation.mi.probe", "version": "1.0.0", "name": "MI",
            "providerId": "app.mediainfo", "providerKind": "plugin", "capability": "probe", "status": "validated",
            "runtime": {"kind": "script", "interpreter": "sh"},
            "exposes": {"namespace": "mediainfo", "contractVersion": "1.0.0",
                        "variables": [{"name": "video.codec", "path": "video.codec"}]}
        }))?;
        let statuses = requirement_statuses(
            &[
                requirement("mediainfo.video.codec", "app.mediainfo"),
                requirement("mediainfo.video.codec", "app.autre"),
            ],
            &registry,
        );
        assert_eq!(
            statuses
                .iter()
                .map(|status| status.available)
                .collect::<Vec<_>>(),
            [true, false]
        );
        Ok(())
    }

    #[test]
    fn declaration_issues_are_reported_for_operations_only() {
        let valid = json!({"providerId": "p", "runtime": {"kind": "script"},
            "exposes": {"namespace": "mediainfo", "contractVersion": "1.0.0", "variables": []}});
        assert_eq!(operation_declaration_issue(&valid.to_string()), None);

        let bad_namespace = json!({"providerId": "p", "runtime": {},
            "exposes": {"namespace": "nodes", "contractVersion": "1.0.0", "variables": []}});
        assert!(
            operation_declaration_issue(&bad_namespace.to_string())
                .is_some_and(|m| m.contains("réservé"))
        );

        let bad_requires = json!({"providerId": "p", "runtime": {"requires": [{"variable": "codec", "provider": "x"}]}});
        assert!(operation_declaration_issue(&bad_requires.to_string()).is_some());

        let bad_variables =
            json!({"providerId": "p", "runtime": {"variables": [{"name": "Mauvais"}]}});
        assert!(
            operation_declaration_issue(&bad_variables.to_string())
                .is_some_and(|m| m.contains("invalide"))
        );

        let unreadable = json!({"providerId": "p", "runtime": {}, "exposes": {"namespace": 3}});
        assert!(
            operation_declaration_issue(&unreadable.to_string())
                .is_some_and(|m| m.contains("illisible"))
        );

        assert_eq!(
            operation_declaration_issue(r#"{"id":"manifeste","files":[]}"#),
            None
        );
        assert_eq!(operation_declaration_issue("pas du json"), None);
    }
}
