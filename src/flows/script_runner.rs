//! Exécuteur agnostique de scripts/exécutables via protocole `stdin`/`stdout` JSON (`kind: "script"`).

use crate::{
    flows::{
        binary_runner::tool_search_path,
        contract::{OperationContract, ScriptInterpreter, ScriptRuntime, merge_params},
        variables::secret_values,
    },
    supervision::SupervisedChild,
    tool_run::{
        MAX_STDOUT_CAPTURE_BYTES, ProgressParser, TailBuffer, for_each_segment, mask_log_secrets,
        read_bounded_into,
    },
    tools_cache::ToolCacheManager,
};
use forgearr_shared::{
    flows::{FlowVariables, SCRIPT_SCHEMA_VERSION},
    jobs::JobErrorCode,
    script_variables::SECRET_PLACEHOLDER,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::{io::AsyncWriteExt, process::Command};

const DEFAULT_SAFE_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
const MAX_SCRIPT_ARGS: usize = 64;
/// Attente des sorties d'un script déjà terminé : un descendant resté en arrière-plan peut garder
/// ses tubes ouverts, ce qui ne doit ni retenir la tâche ni masquer le résultat du script.
const PIPE_DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Callback de progression émis pendant l'exécution d'un script.
pub type ScriptProgressCallback = Arc<dyn Fn(u32, &str) + Send + Sync>;

/// Statut retourné par un script sur `stdout` JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptOutcomeStatus {
    #[default]
    Ok,
    Skip,
    Error,
}

/// Charge utile JSON envoyée au script sur `stdin`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInput<'a> {
    pub schema_version: &'static str,
    pub flow_id: &'a str,
    pub node_id: &'a str,
    pub operation_id: &'a str,
    pub working_file: String,
    /// Fichier d'origine de la tâche (référence pour une comparaison, ex. VMAF).
    pub source_file: String,
    pub work_dir: String,
    pub media_info: &'a Value,
    pub config: Value,
    /// Variables du flux : `{"file": {..}, "job": {..}, "vars": {..}}`.
    pub variables: Value,
}

/// Réponse JSON optionnelle lue sur `stdout` après exécution du script.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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

/// Issue d'une étape script exécutée avec succès ou ignorée.
#[derive(Clone, Debug)]
pub enum ScriptStepOutcome {
    Succeeded {
        next_working_file: Option<PathBuf>,
        updated_media_info: Option<Value>,
        next_port: Option<String>,
        stop_pipeline: bool,
        /// Variables renvoyées par le script, à fusionner dans le contexte.
        variables: Option<Map<String, Value>>,
        message: Option<String>,
        excerpt: Option<String>,
    },
    Skipped {
        message: String,
        excerpt: Option<String>,
        /// Port demandé par le script (`nextPort`), résolu ensuite selon le mode de sortie.
        next_port: Option<String>,
    },
}

/// Erreur d'exécution d'un script agnostique.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ScriptStepError {
    pub code: JobErrorCode,
    pub message: String,
    pub excerpt: Option<String>,
}

struct TempScriptFile(PathBuf);

impl Drop for TempScriptFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn sanitize_script_filename(entrypoint: Option<&str>, step_index: i32) -> String {
    let base = entrypoint
        .and_then(|p| Path::new(p).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("script.txt");
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let suffix = clean.trim_start_matches('.');
    let suffix = if suffix.is_empty() {
        "script.txt"
    } else {
        suffix
    };
    format!(".flux-script-{step_index}-{suffix}")
}

fn resolve_program(
    cache_mgr: &ToolCacheManager,
    rt: &ScriptRuntime,
    interp_override: Option<ScriptInterpreter>,
    tool_versions: &HashMap<String, String>,
) -> Result<PathBuf, ScriptStepError> {
    let error_code = rt.error_code.unwrap_or(JobErrorCode::OperationFailed);
    let effective_interp = interp_override.or(rt.interpreter);
    if let Some(tool) = &rt.tool {
        let version = tool_versions
            .get(&tool.id)
            .map(String::as_str)
            .unwrap_or(&tool.version);
        let bin_name = interp_override
            .map(ScriptInterpreter::executable)
            .or(rt.executable.as_deref())
            .or_else(|| effective_interp.map(ScriptInterpreter::executable))
            .unwrap_or(&tool.id);
        let cached = cache_mgr.resolve_executable(&tool.id, version, bin_name, "");
        if cached.is_file() || effective_interp.is_none() {
            return Ok(cached);
        }
    }
    if let Some(interp) = effective_interp {
        return Ok(PathBuf::from(interp.executable()));
    }
    Err(ScriptStepError {
        code: error_code,
        message: "aucun interpréteur ni outil configuré pour le script".into(),
        excerpt: None,
    })
}

/// Écrit l'entrée du script puis ferme son stdin. Sous le délai du script : un script qui ne lit
/// pas son entrée, plus grosse que le tampon du tube, ne doit pas bloquer la tâche sans fin. Un
/// tube fermé (script déjà terminé) n'est pas une erreur.
async fn send_stdin(stdin: Option<tokio::process::ChildStdin>, bytes: &[u8]) {
    let Some(mut stdin) = stdin else {
        return;
    };
    let _ = stdin.write_all(bytes).await;
    let _ = stdin.shutdown().await;
}

fn resolve_script_working_file(work_dir: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("workingFile retourné vide par le script".into());
    }
    let candidate = Path::new(trimmed);
    let joined = if candidate.is_relative() {
        work_dir.join(candidate)
    } else {
        candidate.to_path_buf()
    };
    let canonical_root =
        std::fs::canonicalize(work_dir).map_err(|err| format!("work_dir inaccessible : {err}"))?;
    let canonical_file = std::fs::canonicalize(&joined)
        .map_err(|err| format!("fichier de sortie du script introuvable : {err}"))?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err("workingFile hors du répertoire de travail interdit".into());
    }
    if !canonical_file.is_file() {
        return Err("workingFile retourné par le script n'est pas un fichier".into());
    }
    Ok(canonical_file)
}

fn merge_media_info(current: &Value, patch: Value) -> Value {
    match (current, patch) {
        (Value::Object(base), Value::Object(updates)) => {
            let mut merged = base.clone();
            for (k, v) in updates {
                merged.insert(k, v);
            }
            Value::Object(merged)
        }
        (_, replacement) => replacement,
    }
}

/// Remplace chaque valeur secrète par un marqueur : un script qui écrit un secret dans son message,
/// ses journaux ou sa progression ne le diffuse ni dans le flux ni dans les journaux.
fn redact(text: &str, secrets: &[String]) -> String {
    secrets.iter().fold(text.to_owned(), |clean, secret| {
        clean.replace(secret.as_str(), SECRET_PLACEHOLDER)
    })
}

/// Message d'un script : secrets retirés, puis jetons et identifiants d'URL masqués.
fn scrub(text: &str, secrets: &[String]) -> String {
    mask_log_secrets(&redact(text, secrets))
}

/// Exécute une opération `kind: "script"` en échangeant du JSON via `stdin`/`stdout`.
#[allow(clippy::too_many_arguments)]
pub async fn execute_script_operation(
    op: &OperationContract,
    rt: &ScriptRuntime,
    cache_mgr: &ToolCacheManager,
    tool_versions: &HashMap<String, String>,
    flow_id: &str,
    node_id: &str,
    node_config: &Value,
    work_dir: &Path,
    working_file: &Path,
    source_file: &Path,
    media_info: &Value,
    variables: &FlowVariables,
    step_index: i32,
    on_progress: ScriptProgressCallback,
) -> Result<ScriptStepOutcome, ScriptStepError> {
    let error_code = rt.error_code.unwrap_or(JobErrorCode::OperationFailed);
    let secrets = secret_values(&rt.variables, node_config);
    let on_progress: ScriptProgressCallback = {
        let secrets = secrets.clone();
        Arc::new(move |percent, line| on_progress(percent, &redact(line, &secrets)))
    };
    let interp_override = node_config
        .get("interpreter")
        .and_then(Value::as_str)
        .and_then(ScriptInterpreter::from_executable);
    let program = resolve_program(cache_mgr, rt, interp_override, tool_versions)?;
    if rt.args.len() > MAX_SCRIPT_ARGS {
        return Err(ScriptStepError {
            code: error_code,
            message: "trop d'arguments déclarés pour le script".into(),
            excerpt: None,
        });
    }

    let inline_code = rt.code.as_deref().or_else(|| {
        node_config
            .get("code")
            .and_then(Value::as_str)
            .filter(|c| !c.trim().is_empty())
    });
    let mut args = rt.args.clone();
    let _temp_guard = match inline_code {
        Some(code) => {
            let fallback_name = interp_override
                .or(rt.interpreter)
                .map(|i| format!("run.{}", i.default_extension()));
            let script_path = work_dir.join(sanitize_script_filename(
                rt.entrypoint.as_deref().or(fallback_name.as_deref()),
                step_index,
            ));
            std::fs::write(&script_path, code).map_err(|err| ScriptStepError {
                code: error_code,
                message: format!("écriture du script impossible : {err}"),
                excerpt: None,
            })?;
            args.push(script_path.to_string_lossy().into_owned());
            Some(TempScriptFile(script_path))
        }
        None if rt.tool.is_some() => None,
        None => {
            return Err(ScriptStepError {
                code: error_code,
                message: format!(
                    "code du script indisponible (entrypoint {:?})",
                    rt.entrypoint
                ),
                excerpt: None,
            });
        }
    };

    let merged_config = Value::Object(merge_params(&rt.defaults, node_config));
    let payload = ScriptInput {
        schema_version: SCRIPT_SCHEMA_VERSION,
        flow_id,
        node_id,
        operation_id: &op.id,
        working_file: working_file.to_string_lossy().into_owned(),
        source_file: source_file.to_string_lossy().into_owned(),
        work_dir: work_dir.to_string_lossy().into_owned(),
        media_info,
        config: merged_config,
        variables: variables.as_document(),
    };
    let stdin_bytes = serde_json::to_vec(&payload).map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("sérialisation JSON d'entrée impossible : {err}"),
        excerpt: None,
    })?;

    let safe_path = std::env::var_os("PATH").unwrap_or_else(|| DEFAULT_SAFE_PATH.into());
    let safe_path = tool_search_path(
        cache_mgr,
        tool_versions,
        &rt.path_tools,
        Some(safe_path.clone()),
    )
    .unwrap_or(safe_path);
    let mut command = Command::new(&program);
    command
        .args(&args)
        .current_dir(work_dir)
        .env_clear()
        .env("PATH", safe_path)
        .env("TMPDIR", work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = SupervisedChild::spawn(&mut command).map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("lancement du script « {} » impossible : {err}", op.id),
        excerpt: None,
    })?;

    let stdin = child.take_stdin();

    let stdout_reader = child.take_stdout().ok_or_else(|| ScriptStepError {
        code: error_code,
        message: "stdout indisponible pour le script".into(),
        excerpt: None,
    })?;
    let stderr_reader = child.take_stderr().ok_or_else(|| ScriptStepError {
        code: error_code,
        message: "stderr indisponible pour le script".into(),
        excerpt: None,
    })?;

    let timeout = Duration::from_secs(rt.timeout_seconds.max(1));
    let mut stdout_bytes = Vec::new();
    let mut stdout_res = Ok(());
    let mut tail = TailBuffer::default();
    let (exited_tx, exited_rx) = tokio::sync::oneshot::channel::<()>();
    let outcome = tokio::time::timeout(timeout, async {
        let io = async {
            tokio::join!(
                read_bounded_into(stdout_reader, &mut stdout_bytes),
                for_each_segment(stderr_reader, |segment| {
                    tail.push(segment);
                    if let Some((pct, line)) =
                        ProgressParser::PercentLine.parse_progress(segment, None)
                    {
                        on_progress(pct, &line);
                    }
                }),
                send_stdin(stdin, &stdin_bytes),
            )
        };
        // Les sorties se lisent jusqu'à leur fin, ou PIPE_DRAIN_GRACE après la sortie du script.
        let drain = async {
            let _ = exited_rx.await;
            tokio::time::sleep(PIPE_DRAIN_GRACE).await;
        };
        let read = async {
            tokio::select! {
                (read, (), ()) = io => stdout_res = read,
                () = drain => {}
            }
        };
        let wait = async {
            let status = child.wait().await;
            let _ = exited_tx.send(());
            status
        };
        let (status, ()) = tokio::join!(wait, read);
        status
    })
    .await;

    let status_res = match outcome {
        Ok(status) => status,
        Err(_) => {
            // Le groupe est tué à l'abandon de `child`.
            return Err(ScriptStepError {
                code: error_code,
                message: format!("délai dépassé pour le script « {} »", op.id),
                excerpt: None,
            });
        }
    };

    let excerpt = tail.finish().map(|text| redact(&text, &secrets));
    let status = status_res.map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("attente du script « {} » impossible : {err}", op.id),
        excerpt: excerpt.clone(),
    })?;
    child.finished();
    stdout_res.map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("lecture stdout du script impossible : {err}"),
        excerpt: excerpt.clone(),
    })?;
    let oversize = stdout_bytes.len() > MAX_STDOUT_CAPTURE_BYTES;
    let stdout_text = String::from_utf8_lossy(&stdout_bytes);
    let trimmed = stdout_text.trim();
    let parsed = if trimmed.is_empty() || oversize {
        None
    } else {
        Some(serde_json::from_str::<ScriptOutput>(trimmed))
    };

    // Le code de sortie prime : un script qui plante n'écrit pas forcément du JSON, et son échec
    // ne doit pas être remplacé par « sortie JSON invalide ».
    if !status.success() {
        let msg = parsed
            .and_then(Result::ok)
            .and_then(|out| out.message)
            .map(|m| scrub(&m, &secrets))
            .unwrap_or_else(|| format!("script « {} » en échec (code {:?})", op.id, status.code()));
        return Err(ScriptStepError {
            code: error_code,
            message: msg,
            excerpt,
        });
    }

    if oversize {
        return Err(ScriptStepError {
            code: error_code,
            message: format!("sortie stdout du script « {} » trop volumineuse", op.id),
            excerpt,
        });
    }
    let parsed_output = match parsed.transpose() {
        Ok(output) => output,
        Err(err) => {
            return Err(ScriptStepError {
                code: error_code,
                message: format!("sortie JSON du script invalide : {err}"),
                excerpt,
            });
        }
    };

    let Some(output) = parsed_output else {
        return Ok(ScriptStepOutcome::Succeeded {
            next_working_file: None,
            updated_media_info: None,
            next_port: None,
            stop_pipeline: false,
            variables: None,
            message: None,
            excerpt,
        });
    };

    match output.status {
        ScriptOutcomeStatus::Error => Err(ScriptStepError {
            code: error_code,
            message: output
                .message
                .map(|m| scrub(&m, &secrets))
                .unwrap_or_else(|| format!("le script « {} » a signalé une erreur", op.id)),
            excerpt,
        }),
        ScriptOutcomeStatus::Skip => Ok(ScriptStepOutcome::Skipped {
            message: output
                .message
                .map(|m| scrub(&m, &secrets))
                .unwrap_or_else(|| "étape ignorée par le script".into()),
            excerpt,
            next_port: output.next_port,
        }),
        ScriptOutcomeStatus::Ok => {
            let next_working_file = match output.working_file {
                Some(ref raw_path) => Some(
                    resolve_script_working_file(work_dir, raw_path).map_err(|err| {
                        ScriptStepError {
                            code: error_code,
                            message: err,
                            excerpt: excerpt.clone(),
                        }
                    })?,
                ),
                None => None,
            };
            let updated_media_info = output
                .media_info
                .map(|patch| merge_media_info(media_info, patch));
            Ok(ScriptStepOutcome::Succeeded {
                next_working_file,
                updated_media_info,
                next_port: output.next_port,
                stop_pipeline: output.stop_pipeline,
                variables: output.variables,
                message: output.message.map(|m| scrub(&m, &secrets)),
                excerpt,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::OperationRuntime;

    fn test_op(code: &str, timeout_seconds: u64) -> (OperationContract, ScriptRuntime) {
        let rt = ScriptRuntime {
            tool: None,
            path_tools: Vec::new(),
            executable: None,
            interpreter: Some(ScriptInterpreter::Sh),
            entrypoint: Some("test.sh".into()),
            code: Some(code.into()),
            args: Vec::new(),
            error_code: None,
            timeout_seconds,
            defaults: serde_json::Map::new(),
            variables: Vec::new(),
            requires: Vec::new(),
            dependencies_unverified: false,
            accepts_non_media: false,
        };
        let op = OperationContract {
            schema_version: "1.1.0".into(),
            id: "operation.script.test".into(),
            version: "1.0.0".into(),
            name: "Script test".into(),
            provider_id: "script.test".into(),
            provider_kind: "script".into(),
            capability: "custom".into(),
            status: "validated".into(),
            runtime: OperationRuntime::Script(rt.clone()),
            exposes: None,
        };
        (op, rt)
    }

    #[tokio::test]
    async fn a_script_ignoring_a_large_stdin_times_out_instead_of_blocking() -> anyhow::Result<()> {
        // Bien au-delà du tampon d'un tube (64 Kio) : l'écriture bloque tant que rien ne lit.
        const PAYLOAD_BYTES: usize = 300_000;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let (op, rt) = test_op("sleep 30", 1);
        let config = serde_json::json!({ "blob": "x".repeat(PAYLOAD_BYTES) });
        let limit = Duration::from_secs(10);
        let outcome = tokio::time::timeout(
            limit,
            execute_script_operation(
                &op,
                &rt,
                &cache,
                &HashMap::new(),
                "flow-1",
                "node-1",
                &config,
                dir.path(),
                &source,
                &source,
                &Value::Null,
                &FlowVariables::default(),
                1,
                Arc::new(|_, _| {}),
            ),
        )
        .await?;
        match outcome {
            Err(error) => assert!(error.message.contains("délai dépassé"), "{}", error.message),
            Ok(_) => anyhow::bail!("le script sans lecture de stdin doit dépasser son délai"),
        }
        Ok(())
    }

    /// Lance `code` (sh) avec un délai de 10 s, sans configuration ni variables.
    async fn run_sh(code: &str) -> anyhow::Result<Result<ScriptStepOutcome, ScriptStepError>> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let (op, rt) = test_op(code, 10);
        Ok(execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &Value::Null,
            dir.path(),
            &source,
            &source,
            &Value::Null,
            &FlowVariables::default(),
            1,
            Arc::new(|_, _| {}),
        )
        .await)
    }

    #[tokio::test]
    async fn a_crashing_script_reports_its_exit_code_and_not_invalid_json() -> anyhow::Result<()> {
        let outcome = run_sh("cat >/dev/null\necho 'Traceback: boom'\nexit 3").await?;
        let Err(error) = outcome else {
            anyhow::bail!("un script en échec doit échouer");
        };
        assert!(error.message.contains("en échec"), "{}", error.message);
        assert!(!error.message.contains("JSON"), "{}", error.message);
        Ok(())
    }

    #[tokio::test]
    async fn a_successful_script_with_invalid_json_still_fails_on_the_json() -> anyhow::Result<()> {
        let outcome = run_sh("cat >/dev/null\necho 'pas du json'").await?;
        let Err(error) = outcome else {
            anyhow::bail!("une sortie illisible doit échouer");
        };
        assert!(error.message.contains("JSON"), "{}", error.message);
        Ok(())
    }

    #[tokio::test]
    async fn a_background_child_holding_the_pipes_does_not_delay_the_result() -> anyhow::Result<()>
    {
        // Le descendant garde stdout et stderr ouverts bien après la fin du script.
        const BACKGROUND_SECONDS: u64 = 6;
        const LIMIT: Duration = Duration::from_secs(5);
        let code = format!(
            "cat >/dev/null\nsleep {BACKGROUND_SECONDS} &\nprintf '{{\"status\":\"ok\",\"message\":\"fini\"}}'"
        );
        let outcome = tokio::time::timeout(LIMIT, run_sh(&code)).await??;
        match outcome {
            Ok(ScriptStepOutcome::Succeeded { message, .. }) => {
                assert_eq!(message.as_deref(), Some("fini"));
            }
            _ => anyhow::bail!("le script a réussi malgré son descendant"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn script_sees_path_tools_first_and_receives_the_source_file() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let tool_bin = cache
            .tool_dir("outil-test", "1.0.0", crate::tools_cache::host_arch())
            .join("bin");
        std::fs::create_dir_all(&tool_bin)?;
        let tool = tool_bin.join("outil-test");
        std::fs::write(&tool, "#!/bin/sh\nprintf outil-du-cache\n")?;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755))?;
        let working = dir.path().join("encode.mkv");
        let original = dir.path().join("original.mkv");
        std::fs::write(&working, b"enc")?;
        std::fs::write(&original, b"orig")?;
        let script = "cat > entree.json\nsrc=$(grep -c '\"sourceFile\":\"[^\"]*original.mkv\"' entree.json)\nprintf '{\"status\":\"ok\",\"message\":\"%s %s\"}' \"$(outil-test)\" \"$src\"";
        let (op, mut rt) = test_op(script, 10);
        rt.path_tools = vec![crate::flows::contract::ToolRef {
            id: "outil-test".into(),
            version: "1.0.0".into(),
        }];

        let outcome = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &Value::Null,
            dir.path(),
            &working,
            &original,
            &Value::Null,
            &FlowVariables::default(),
            1,
            Arc::new(|_, _| {}),
        )
        .await?;
        match outcome {
            ScriptStepOutcome::Succeeded { message, .. } => {
                assert_eq!(message.as_deref(), Some("outil-du-cache 1"));
            }
            ScriptStepOutcome::Skipped { .. } => anyhow::bail!("étape ignorée inattendue"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn script_exchanges_json_over_stdio_and_routes_port() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let out_file = dir.path().join("renamed.mkv");
        let script = format!(
            "cat >/dev/null\necho 'Progress: 50%' >&2\ncp \"{}\" \"{}\"\nprintf '%s' '{{\"status\":\"ok\",\"nextPort\":\"branch_a\",\"workingFile\":\"renamed.mkv\",\"mediaInfo\":{{\"customFlag\":true}},\"message\":\"script ok\"}}'",
            source.display(),
            out_file.display()
        );
        let (op, rt) = test_op(&script, 10);
        let progress = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
        let progress_clone = progress.clone();
        let cb: ScriptProgressCallback = Arc::new(move |pct, _| {
            if let Ok(mut guard) = progress_clone.lock() {
                guard.push(pct);
            }
        });

        let outcome = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &serde_json::json!({"threshold": 42}),
            dir.path(),
            &source,
            &source,
            &serde_json::json!({"format": {"duration": "10"}}),
            &FlowVariables::default(),
            1,
            cb,
        )
        .await?;

        match outcome {
            ScriptStepOutcome::Succeeded {
                next_working_file,
                updated_media_info,
                next_port,
                stop_pipeline,
                variables,
                message,
                excerpt,
            } => {
                assert!(variables.is_none());
                assert_eq!(next_working_file, Some(std::fs::canonicalize(&out_file)?));
                assert_eq!(next_port.as_deref(), Some("branch_a"));
                assert!(!stop_pipeline);
                assert_eq!(message.as_deref(), Some("script ok"));
                assert!(excerpt.as_deref().is_some_and(|e| e.contains("50%")));
                let info =
                    updated_media_info.ok_or_else(|| anyhow::anyhow!("mediaInfo manquant"))?;
                assert_eq!(info["customFlag"], true);
                assert_eq!(info["format"]["duration"], "10");
            }
            ScriptStepOutcome::Skipped { .. } => panic!("attendu Succeeded"),
        }
        assert!(
            progress
                .lock()
                .map_err(|_| anyhow::anyhow!("lock"))?
                .contains(&50)
        );
        Ok(())
    }

    #[tokio::test]
    async fn skip_carries_next_port() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let (op, rt) = test_op(
            "cat >/dev/null\nprintf '%s' '{\"status\":\"skip\",\"nextPort\":\"true\"}'",
            10,
        );
        let outcome = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &serde_json::json!({}),
            dir.path(),
            &source,
            &source,
            &serde_json::json!({}),
            &FlowVariables::default(),
            1,
            Arc::new(|_, _| {}),
        )
        .await?;
        match outcome {
            ScriptStepOutcome::Skipped { next_port, .. } => {
                assert_eq!(next_port.as_deref(), Some("true"));
            }
            ScriptStepOutcome::Succeeded { .. } => anyhow::bail!("attendu Skipped"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn script_rejects_working_file_outside_work_dir() -> anyhow::Result<()> {
        let work_dir = tempfile::tempdir()?;
        let outside_dir = tempfile::tempdir()?;
        let outside_file = outside_dir.path().join("evil.mkv");
        std::fs::write(&outside_file, b"x")?;
        let source = work_dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let cache = ToolCacheManager::new(work_dir.path().join("cache"));

        let script = format!(
            "cat >/dev/null\nprintf '{{\"status\":\"ok\",\"workingFile\":\"{}\"}}'",
            outside_file.display()
        );
        let (op, rt) = test_op(&script, 10);
        let res = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &Value::Null,
            work_dir.path(),
            &source,
            &source,
            &Value::Null,
            &FlowVariables::default(),
            1,
            Arc::new(|_, _| {}),
        )
        .await;
        assert!(matches!(res, Err(err) if err.message.contains("hors du répertoire")));
        Ok(())
    }

    #[tokio::test]
    async fn script_clears_parent_environment_secrets() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        // Vérifie que `HOME` / variables non listées ne sont pas héritées par le sous-processus.
        let script = r#"
cat >/dev/null
if [ -n "$HOME" ]; then
  printf '{"status":"error","message":"HOME leaked"}'
  exit 1
fi
printf '{"status":"ok","message":"isolated"}'
"#;
        let (op, rt) = test_op(script, 10);
        let outcome = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &Value::Null,
            dir.path(),
            &source,
            &source,
            &Value::Null,
            &FlowVariables::default(),
            1,
            Arc::new(|_, _| {}),
        )
        .await?;
        assert!(matches!(
            outcome,
            ScriptStepOutcome::Succeeded {
                message: Some(ref m),
                ..
            } if m == "isolated"
        ));
        Ok(())
    }

    #[test]
    fn output_of_a_1_0_script_has_no_variables() -> Result<(), serde_json::Error> {
        let output: ScriptOutput = serde_json::from_str(r#"{"status":"ok","nextPort":"a"}"#)?;
        assert!(output.variables.is_none());
        assert_eq!(output.next_port.as_deref(), Some("a"));
        Ok(())
    }

    #[tokio::test]
    async fn script_receives_and_returns_flow_variables() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let mut variables = FlowVariables::default();
        variables.set_custom("codec", serde_json::json!("hevc"))?;
        // Le script relit `vars.codec` sur stdin et renvoie une nouvelle variable.
        let script = r#"
seen=$(grep -c '"vars":{"codec":"hevc"}' -)
printf '{"status":"ok","message":"vu %s","variables":{"cible":"av1"}}' "$seen"
"#;
        let (op, rt) = test_op(script, 10);
        let outcome = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &Value::Null,
            dir.path(),
            &source,
            &source,
            &Value::Null,
            &variables,
            1,
            Arc::new(|_, _| {}),
        )
        .await?;
        match outcome {
            ScriptStepOutcome::Succeeded {
                variables, message, ..
            } => {
                assert_eq!(message.as_deref(), Some("vu 1"));
                let returned = variables.ok_or_else(|| anyhow::anyhow!("variables absentes"))?;
                assert_eq!(returned["cible"], "av1");
            }
            ScriptStepOutcome::Skipped { .. } => anyhow::bail!("étape ignorée inattendue"),
        }
        Ok(())
    }

    #[test]
    fn redaction_replaces_every_occurrence_of_each_secret() {
        let secrets = vec!["t0k".to_owned(), "autre".to_owned()];
        assert_eq!(
            redact("t0k puis autre puis t0k", &secrets),
            format!("{SECRET_PLACEHOLDER} puis {SECRET_PLACEHOLDER} puis {SECRET_PLACEHOLDER}")
        );
        assert_eq!(redact("rien à cacher", &secrets), "rien à cacher");
        assert_eq!(redact("texte", &[]), "texte");
    }

    #[tokio::test]
    async fn a_script_receives_its_secret_but_cannot_leak_it_in_its_message_or_logs()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let source = dir.path().join("input.mkv");
        std::fs::write(&source, b"mkv")?;
        let script = "cat > entree.json\nif grep -q '\"jeton\":\"t0k-secret\"' entree.json; then\necho 'Progress: 50% avec t0k-secret' >&2\nprintf '{\"status\":\"ok\",\"message\":\"recu t0k-secret\"}'\nelse\nprintf '{\"status\":\"error\",\"message\":\"secret absent\"}'\nfi";
        let (op, mut rt) = test_op(script, 10);
        rt.variables = vec![forgearr_shared::script_variables::ScriptVariable {
            name: "jeton".into(),
            kind: forgearr_shared::script_variables::VariableKind::Secret,
            ..Default::default()
        }];
        let lines = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let sink = lines.clone();
        let outcome = execute_script_operation(
            &op,
            &rt,
            &cache,
            &HashMap::new(),
            "flow-1",
            "node-1",
            &serde_json::json!({"jeton": "t0k-secret"}),
            dir.path(),
            &source,
            &source,
            &Value::Null,
            &FlowVariables::default(),
            1,
            Arc::new(move |_, line| {
                if let Ok(mut guard) = sink.lock() {
                    guard.push(line.to_owned());
                }
            }),
        )
        .await?;
        let ScriptStepOutcome::Succeeded { message, .. } = outcome else {
            anyhow::bail!("le script aurait dû réussir : il reçoit son secret");
        };
        assert_eq!(
            message.as_deref(),
            Some(format!("recu {SECRET_PLACEHOLDER}").as_str())
        );
        let logged = lines
            .lock()
            .map(|guard| guard.join("|"))
            .unwrap_or_default();
        assert!(!logged.contains("t0k-secret"), "{logged}");
        Ok(())
    }
}
