//! Exécuteur agnostique de scripts/exécutables via protocole `stdin`/`stdout` JSON (`kind: "script"`).

use crate::{
    flows::contract::{OperationContract, ScriptInterpreter, ScriptRuntime, merge_params},
    tool_run::{
        MAX_STDOUT_CAPTURE_BYTES, ProgressParser, TailBuffer, for_each_segment, mask_log_secrets,
        read_bounded_bytes,
    },
    tools_cache::ToolCacheManager,
};
use flux_shared::jobs::JobErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
const SCRIPT_SCHEMA_VERSION: &str = "1.0";
const MAX_SCRIPT_ARGS: usize = 64;

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
    pub work_dir: String,
    pub media_info: &'a Value,
    pub config: Value,
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
}

/// Issue d'une étape script exécutée avec succès ou ignorée.
#[derive(Clone, Debug)]
pub enum ScriptStepOutcome {
    Succeeded {
        next_working_file: Option<PathBuf>,
        updated_media_info: Option<Value>,
        next_port: Option<String>,
        stop_pipeline: bool,
        message: Option<String>,
        excerpt: Option<String>,
    },
    Skipped {
        message: String,
        excerpt: Option<String>,
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
    media_info: &Value,
    step_index: i32,
    on_progress: ScriptProgressCallback,
) -> Result<ScriptStepOutcome, ScriptStepError> {
    let error_code = rt.error_code.unwrap_or(JobErrorCode::OperationFailed);
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
        work_dir: work_dir.to_string_lossy().into_owned(),
        media_info,
        config: merged_config,
    };
    let stdin_bytes = serde_json::to_vec(&payload).map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("sérialisation JSON d'entrée impossible : {err}"),
        excerpt: None,
    })?;

    let safe_path = std::env::var("PATH").unwrap_or_else(|_| DEFAULT_SAFE_PATH.to_owned());
    let mut child = Command::new(&program)
        .args(&args)
        .current_dir(work_dir)
        .env_clear()
        .env("PATH", safe_path)
        .env("TMPDIR", work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| ScriptStepError {
            code: error_code,
            message: format!("lancement du script « {} » impossible : {err}", op.id),
            excerpt: None,
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&stdin_bytes).await;
        let _ = stdin.shutdown().await;
    }

    let stdout_reader = child.stdout.take().ok_or_else(|| ScriptStepError {
        code: error_code,
        message: "stdout indisponible pour le script".into(),
        excerpt: None,
    })?;
    let stderr_reader = child.stderr.take().ok_or_else(|| ScriptStepError {
        code: error_code,
        message: "stderr indisponible pour le script".into(),
        excerpt: None,
    })?;

    let timeout = Duration::from_secs(rt.timeout_seconds.max(1));
    let outcome = tokio::time::timeout(timeout, async {
        let mut tail = TailBuffer::default();
        let (stdout_res, (), status_res) = tokio::join!(
            read_bounded_bytes(stdout_reader),
            for_each_segment(stderr_reader, |segment| {
                tail.push(segment);
                if let Some((pct, line)) = ProgressParser::PercentLine.parse_progress(segment, None)
                {
                    on_progress(pct, &line);
                }
            }),
            child.wait(),
        );
        (stdout_res, tail.finish(), status_res)
    })
    .await;

    let (stdout_res, excerpt, status_res) = match outcome {
        Ok(res) => res,
        Err(_) => {
            let _ = child.kill().await;
            return Err(ScriptStepError {
                code: error_code,
                message: format!("délai dépassé pour le script « {} »", op.id),
                excerpt: None,
            });
        }
    };

    let status = status_res.map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("attente du script « {} » impossible : {err}", op.id),
        excerpt: excerpt.clone(),
    })?;
    let stdout_bytes = stdout_res.map_err(|err| ScriptStepError {
        code: error_code,
        message: format!("lecture stdout du script impossible : {err}"),
        excerpt: excerpt.clone(),
    })?;
    if stdout_bytes.len() > MAX_STDOUT_CAPTURE_BYTES {
        return Err(ScriptStepError {
            code: error_code,
            message: format!("sortie stdout du script « {} » trop volumineuse", op.id),
            excerpt,
        });
    }

    let stdout_text = String::from_utf8_lossy(&stdout_bytes);
    let trimmed = stdout_text.trim();
    let parsed_output: Option<ScriptOutput> = if trimmed.is_empty() {
        None
    } else {
        match serde_json::from_str::<ScriptOutput>(trimmed) {
            Ok(out) => Some(out),
            Err(err) => {
                return Err(ScriptStepError {
                    code: error_code,
                    message: format!("sortie JSON du script invalide : {err}"),
                    excerpt,
                });
            }
        }
    };

    if !status.success() {
        let msg = parsed_output
            .and_then(|out| out.message)
            .map(|m| mask_log_secrets(&m))
            .unwrap_or_else(|| format!("script « {} » en échec (code {:?})", op.id, status.code()));
        return Err(ScriptStepError {
            code: error_code,
            message: msg,
            excerpt,
        });
    }

    let Some(output) = parsed_output else {
        return Ok(ScriptStepOutcome::Succeeded {
            next_working_file: None,
            updated_media_info: None,
            next_port: None,
            stop_pipeline: false,
            message: None,
            excerpt,
        });
    };

    match output.status {
        ScriptOutcomeStatus::Error => Err(ScriptStepError {
            code: error_code,
            message: output
                .message
                .map(|m| mask_log_secrets(&m))
                .unwrap_or_else(|| format!("le script « {} » a signalé une erreur", op.id)),
            excerpt,
        }),
        ScriptOutcomeStatus::Skip => Ok(ScriptStepOutcome::Skipped {
            message: output
                .message
                .map(|m| mask_log_secrets(&m))
                .unwrap_or_else(|| "étape ignorée par le script".into()),
            excerpt,
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
                message: output.message.map(|m| mask_log_secrets(&m)),
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
            executable: None,
            interpreter: Some(ScriptInterpreter::Sh),
            entrypoint: Some("test.sh".into()),
            code: Some(code.into()),
            args: Vec::new(),
            error_code: None,
            timeout_seconds,
            defaults: serde_json::Map::new(),
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
        };
        (op, rt)
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
            &serde_json::json!({"format": {"duration": "10"}}),
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
                message,
                excerpt,
            } => {
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
            &Value::Null,
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
            &Value::Null,
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
}
