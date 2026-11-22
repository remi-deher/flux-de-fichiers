//! Variables de flux côté moteur : contexte intrinsèque et substitution dans la configuration.

use forgearr_shared::flows::{
    FlowVariables, IntrinsicVariable, RAW_CONFIG_KEYS,
    legacy_references::upgrade_legacy_value,
    template_scope::{TemplateScope, render_in_scope, render_in_scope_keeping_runner_tokens},
};
use serde_json::Value;
use std::{ffi::OsStr, path::Path};

/// Identité d'une exécution, source des variables intrinsèques qui ne dépendent pas du fichier.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunIdentity<'a> {
    pub job_id: Option<&'a str>,
    pub flow_id: Option<&'a str>,
    pub source_name: Option<&'a str>,
    pub library_id: Option<&'a str>,
    pub library_name: Option<&'a str>,
    pub worker_name: Option<&'a str>,
}

fn text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.to_owned()))
}

fn os_text(value: Option<&OsStr>) -> Value {
    value.map_or(Value::Null, |part| {
        Value::String(part.to_string_lossy().into_owned())
    })
}

/// Renseigne les variables intrinsèques d'identité (`job.id`, `library.name`…).
pub fn set_identity_variables(vars: &mut FlowVariables, identity: &RunIdentity<'_>) {
    vars.set_intrinsic(IntrinsicVariable::JobId, text(identity.job_id));
    vars.set_intrinsic(IntrinsicVariable::FlowId, text(identity.flow_id));
    vars.set_intrinsic(IntrinsicVariable::SourceName, text(identity.source_name));
    vars.set_intrinsic(IntrinsicVariable::LibraryId, text(identity.library_id));
    vars.set_intrinsic(IntrinsicVariable::LibraryName, text(identity.library_name));
    vars.set_intrinsic(IntrinsicVariable::WorkerName, text(identity.worker_name));
}

/// Renseigne `error.code` et `error.message` pour l'exécution du flux d'échec.
pub fn set_failure_variables(vars: &mut FlowVariables, code: &str, message: &str) {
    vars.set_intrinsic(IntrinsicVariable::ErrorCode, text(Some(code)));
    vars.set_intrinsic(IntrinsicVariable::ErrorMessage, text(Some(message)));
}

/// Renseigne `source.path` et `work.dir`, fixes pendant toute l'exécution.
pub fn set_location_variables(vars: &mut FlowVariables, source_file: &Path, work_dir: &Path) {
    vars.set_intrinsic(
        IntrinsicVariable::SourcePath,
        os_text(Some(source_file.as_os_str())),
    );
    vars.set_intrinsic(
        IntrinsicVariable::WorkDir,
        os_text(Some(work_dir.as_os_str())),
    );
}

/// Recalcule `file.*` pour le fichier de travail courant (taille nulle s'il est illisible).
pub fn set_file_variables(vars: &mut FlowVariables, file: &Path) {
    vars.set_intrinsic(IntrinsicVariable::FilePath, os_text(Some(file.as_os_str())));
    vars.set_intrinsic(IntrinsicVariable::FileName, os_text(file.file_name()));
    vars.set_intrinsic(IntrinsicVariable::FileStem, os_text(file.file_stem()));
    vars.set_intrinsic(IntrinsicVariable::FileExt, os_text(file.extension()));
    vars.set_intrinsic(
        IntrinsicVariable::FileDir,
        os_text(file.parent().map(Path::as_os_str)),
    );
    let size = std::fs::metadata(file).map_or(Value::Null, |meta| Value::from(meta.len()));
    vars.set_intrinsic(IntrinsicVariable::FileSize, size);
}

/// Applique `render` à chaque valeur de premier niveau, sauf aux clés brutes (`code`, `secrets`),
/// après conversion des anciennes références `{{…}}` des seules racines du moteur (un `{{id}}`
/// destiné à l'outil appelé reste littéral) : une seule passe de rendu par valeur.
fn map_config(config: &Value, render: &dyn Fn(&Value) -> Value) -> Value {
    match &upgrade_legacy_value(config) {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let rendered = if RAW_CONFIG_KEYS.contains(&key.as_str()) {
                        value.clone()
                    } else {
                        render(value)
                    };
                    (key.clone(), rendered)
                })
                .collect(),
        ),
        other => render(other),
    }
}

/// Rend les modèles `{$…}` de la configuration d'un nœud avec la portée de l'étape.
pub fn substitute_node_config(config: &Value, scope: &dyn TemplateScope) -> Value {
    map_config(config, &|value| render_in_scope(value, scope))
}

/// Variante pour les blocs HTTP : les jetons propres au runner (`{$context.*}`,
/// `{$setting.*}`, `{$config.*}`…) et ceux que la portée ne connaît pas sont conservés pour
/// qu'il les résolve lui-même.
pub fn substitute_flow_variables_only(config: &Value, scope: &dyn TemplateScope) -> Value {
    map_config(config, &|value| {
        render_in_scope_keeping_runner_tokens(value, scope)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> FlowVariables {
        let mut vars = FlowVariables::default();
        set_file_variables(&mut vars, Path::new("/media/films/Film.2024.mkv"));
        set_identity_variables(
            &mut vars,
            &RunIdentity {
                library_name: Some("Films"),
                ..RunIdentity::default()
            },
        );
        vars.set_custom("codec", json!("hevc")).unwrap_or_default();
        vars
    }

    #[test]
    fn computes_file_and_identity_variables() {
        let vars = sample();
        assert_eq!(vars.lookup("file.name"), Some(&json!("Film.2024.mkv")));
        assert_eq!(vars.lookup("file.stem"), Some(&json!("Film.2024")));
        assert_eq!(vars.lookup("file.ext"), Some(&json!("mkv")));
        assert_eq!(vars.lookup("file.dir"), Some(&json!("/media/films")));
        assert_eq!(
            vars.lookup("file.path"),
            Some(&json!("/media/films/Film.2024.mkv"))
        );
        assert_eq!(vars.lookup("file.size"), Some(&Value::Null));
        assert_eq!(vars.lookup("library.name"), Some(&json!("Films")));
        assert_eq!(vars.lookup("worker.name"), Some(&Value::Null));
    }

    #[test]
    fn sets_location_variables() {
        let mut vars = FlowVariables::default();
        set_location_variables(&mut vars, Path::new("/in/a.mkv"), Path::new("/work"));
        assert_eq!(vars.lookup("source.path"), Some(&json!("/in/a.mkv")));
        assert_eq!(vars.lookup("work.dir"), Some(&json!("/work")));
    }

    #[test]
    fn legacy_references_are_converted_before_the_single_pass() {
        let vars = sample();
        let config = json!({
            "output": "{{workingFile}}",
            "code": "echo {{workingFile}}",
            "secrets": {"t": "{{workingFile}}"}
        });
        assert_eq!(
            substitute_node_config(&config, &vars),
            json!({
                "output": "/media/films/Film.2024.mkv",
                "code": "echo {{workingFile}}",
                "secrets": {"t": "{{workingFile}}"}
            })
        );
    }

    #[test]
    fn foreign_mustaches_of_a_binary_or_http_node_stay_literal() {
        let vars = sample();
        let config = json!({
            "args": ["--id", "{{id}}", "{{workingFile}}"],
            "body": "{\"tpl\": \"{{user.name}}\"}"
        });
        let expected = json!({
            "args": ["--id", "{{id}}", "/media/films/Film.2024.mkv"],
            "body": "{\"tpl\": \"{{user.name}}\"}"
        });
        assert_eq!(substitute_node_config(&config, &vars), expected);
        assert_eq!(substitute_flow_variables_only(&config, &vars), expected);
    }

    #[test]
    fn sets_failure_variables() {
        let mut vars = FlowVariables::default();
        assert_eq!(vars.lookup("error.code"), None);
        set_failure_variables(&mut vars, "operation_failed", "encodage interrompu");
        assert_eq!(vars.lookup("error.code"), Some(&json!("operation_failed")));
        assert_eq!(
            vars.lookup("error.message"),
            Some(&json!("encodage interrompu"))
        );
    }

    #[test]
    fn measures_an_existing_file() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("a.bin");
        std::fs::write(&file, b"12345")?;
        let mut vars = FlowVariables::default();
        set_file_variables(&mut vars, &file);
        assert_eq!(vars.lookup("file.size"), Some(&json!(5)));
        Ok(())
    }

    #[test]
    fn substitutes_nested_config_but_not_script_code() {
        let vars = sample();
        let config = json!({
            "output": "{$file.stem}-{$vars.codec}.{$file.ext}",
            "nested": {"list": ["{$library.name}"]},
            "inconnu": "{$vars.absente|défaut}",
            "code": "echo '{$file.name}'"
        });
        assert_eq!(
            substitute_node_config(&config, &vars),
            json!({
                "output": "Film.2024-hevc.mkv",
                "nested": {"list": ["Films"]},
                "inconnu": "défaut",
                "code": "echo '{$file.name}'"
            })
        );
    }

    #[test]
    fn http_variant_keeps_runner_tokens() {
        let vars = sample();
        let config = json!({
            "message": "Terminé : {$file.name} ({$context.jobId})",
            "topic": "{$setting.topic|défaut}",
            "codec": "{$vars.codec}",
            "code": "{$file.name}"
        });
        assert_eq!(
            substitute_flow_variables_only(&config, &vars),
            json!({
                "message": "Terminé : Film.2024.mkv ({$context.jobId})",
                "topic": "{$setting.topic|défaut}",
                "codec": "hevc",
                "code": "{$file.name}"
            })
        );
    }

    #[test]
    fn stored_secrets_are_never_rendered() {
        let vars = sample();
        let config = json!({
            "secrets": {"token": "{$file.name}"},
            "output": "{$file.name}"
        });
        let expected = json!({
            "secrets": {"token": "{$file.name}"},
            "output": "Film.2024.mkv"
        });
        assert_eq!(substitute_node_config(&config, &vars), expected);
        assert_eq!(substitute_flow_variables_only(&config, &vars), expected);
    }
}
