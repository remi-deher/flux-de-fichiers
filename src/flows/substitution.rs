//! Variables de flux côté moteur : contexte intrinsèque et substitution dans la configuration.

use forgearr_shared::flows::{
    DEFAULT_SEPARATOR, FlowVariables, IntrinsicVariable, TEMPLATE_CLOSE, TEMPLATE_OPEN,
    TemplateToken, exact_token, render_piece, render_template_with, render_value,
};
use serde_json::Value;
use std::{ffi::OsStr, path::Path};

/// Clés de configuration jamais réécrites : le code d'un script est transmis tel quel.
const RAW_CONFIG_KEYS: [&str; 1] = ["code"];

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
    value.map_or(Value::Null, |part| Value::String(part.to_string_lossy().into_owned()))
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

/// Recalcule `file.*` pour le fichier de travail courant (taille nulle s'il est illisible).
pub fn set_file_variables(vars: &mut FlowVariables, file: &Path) {
    vars.set_intrinsic(IntrinsicVariable::FileName, os_text(file.file_name()));
    vars.set_intrinsic(IntrinsicVariable::FileStem, os_text(file.file_stem()));
    vars.set_intrinsic(IntrinsicVariable::FileExt, os_text(file.extension()));
    vars.set_intrinsic(IntrinsicVariable::FileDir, os_text(file.parent().map(Path::as_os_str)));
    let size = std::fs::metadata(file).map_or(Value::Null, |meta| Value::from(meta.len()));
    vars.set_intrinsic(IntrinsicVariable::FileSize, size);
}

/// Applique `render` à chaque valeur de premier niveau, sauf aux clés brutes (`code`).
fn map_config(config: &Value, render: &dyn Fn(&Value) -> Value) -> Value {
    match config {
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

/// Rend les modèles `{$…}` de la configuration d'un nœud avec les variables du flux.
pub fn substitute_node_config(config: &Value, vars: &FlowVariables) -> Value {
    let resolve = |expr: &str| vars.lookup(expr).cloned();
    map_config(config, &|value| render_value(value, &resolve))
}

/// Réécrit un jeton tel qu'il a été saisi (`{$expr|défaut}`).
fn verbatim_token(token: &TemplateToken<'_>) -> String {
    let default = if token.default.is_empty() {
        String::new()
    } else {
        format!("{DEFAULT_SEPARATOR}{}", token.default)
    };
    format!("{TEMPLATE_OPEN}{}{default}{TEMPLATE_CLOSE}", token.expr)
}

/// Rend une valeur en ne remplaçant que les variables de flux ; les autres jetons restent.
fn render_flow_variables_only(value: &Value, vars: &FlowVariables) -> Value {
    match value {
        Value::String(text) => {
            if let Some(resolved) = exact_token(text)
                .filter(|expr| FlowVariables::is_variable_property(expr))
                .and_then(|expr| vars.lookup(expr))
            {
                return resolved.clone();
            }
            Value::String(render_template_with(text, &mut |token: &TemplateToken<'_>| {
                if FlowVariables::is_variable_property(token.expr) {
                    render_piece(vars.lookup(token.expr), token.default)
                } else {
                    verbatim_token(token)
                }
            }))
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| render_flow_variables_only(item, vars))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), render_flow_variables_only(item, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Variante pour les blocs HTTP : les jetons propres au runner (`{$context.*}`,
/// `{$setting.*}`, `{$config.*}`…) sont conservés pour qu'il les résolve lui-même.
pub fn substitute_flow_variables_only(config: &Value, vars: &FlowVariables) -> Value {
    map_config(config, &|value| render_flow_variables_only(value, vars))
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
        assert_eq!(vars.lookup("file.size"), Some(&Value::Null));
        assert_eq!(vars.lookup("library.name"), Some(&json!("Films")));
        assert_eq!(vars.lookup("worker.name"), Some(&Value::Null));
    }

    #[test]
    fn sets_failure_variables() {
        let mut vars = FlowVariables::default();
        assert_eq!(vars.lookup("error.code"), None);
        set_failure_variables(&mut vars, "operation_failed", "encodage interrompu");
        assert_eq!(vars.lookup("error.code"), Some(&json!("operation_failed")));
        assert_eq!(vars.lookup("error.message"), Some(&json!("encodage interrompu")));
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
}
