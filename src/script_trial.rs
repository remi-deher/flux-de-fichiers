//! Essai d'un script sur ce worker (ADR 0031) : même entrée JSON, même environnement et même interpréteur qu'une
//! exécution réelle, dans un dossier jetable, avec stdin, stdout, stderr et code de sortie rendus tels quels.

use crate::{
    flows::script_runner::{ScriptInput, sanitize_script_filename, send_stdin},
    request_broker::{Relayed, RequestBroker},
    supervision::SupervisedChild,
    tool_run::{mask_log_secrets, read_bounded_into},
};
use forgearr_shared::{
    flows::SCRIPT_SCHEMA_VERSION,
    script_trial::{
        ScriptTrialRequest, ScriptTrialResult, TRIAL_CAPTURE_BYTES, TRIAL_TIMEOUT_SECONDS,
        WorkerScriptTrialReport, WorkerScriptTrialTask, parse_script_response,
    },
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;
use uuid::Uuid;

const DEFAULT_SAFE_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
/// Identifiant donné au flux, au nœud et à l'opération dans l'entrée d'un essai.
const TRIAL_IDENTIFIER: &str = "essai";
const SAMPLE_FILE_NAME: &str = "exemple.mkv";
const SCRATCH_PREFIX: &str = ".script-trial-";
/// Sous-dossier, sous les données du central ou le dossier de travail d'un worker, où s'exécutent les essais.
const TRIAL_DIRECTORY: &str = "script-trials";

impl Relayed for WorkerScriptTrialTask {
    fn request_id(&self) -> Uuid {
        self.request_id
    }
    fn worker_id(&self) -> Uuid {
        self.worker_id
    }
}

impl Relayed for WorkerScriptTrialReport {
    fn request_id(&self) -> Uuid {
        self.request_id
    }
    fn worker_id(&self) -> Uuid {
        self.worker_id
    }
}

/// Courtier des essais confiés à un worker distant (ADR 0031, lot 3b).
pub type TrialBroker = RequestBroker<WorkerScriptTrialTask, WorkerScriptTrialReport>;

/// Dossier des essais sous `parent`, créé au besoin et rendu absolu : le script s'exécute dans son propre
/// dossier, où un chemin relatif ne désignerait plus rien.
pub async fn scratch_root(parent: &Path) -> std::io::Result<PathBuf> {
    let root = parent.join(TRIAL_DIRECTORY);
    tokio::fs::create_dir_all(&root).await?;
    tokio::fs::canonicalize(&root).await
}

/// Dossier de travail de l'essai, supprimé à la fin quel que soit le résultat.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Exécute l'essai sous `scratch_root`, avec le délai de `TRIAL_TIMEOUT_SECONDS`.
pub async fn run_script_trial(
    request: &ScriptTrialRequest,
    scratch_root: &Path,
) -> ScriptTrialResult {
    let limit = Duration::from_secs(TRIAL_TIMEOUT_SECONDS);
    run_script_trial_within(request, scratch_root, limit).await
}

async fn run_script_trial_within(
    request: &ScriptTrialRequest,
    scratch_root: &Path,
    limit: Duration,
) -> ScriptTrialResult {
    let mut result = ScriptTrialResult::default();
    let dir = scratch_root.join(format!("{SCRATCH_PREFIX}{}", Uuid::new_v4()));
    let _scratch = Scratch(dir.clone());
    let prepared = prepare(request, &dir);
    let (script_path, stdin_text) = match prepared {
        Ok(prepared) => prepared,
        Err(message) => {
            result.launch_error = Some(message);
            return result;
        }
    };
    result.stdin = stdin_text.clone();

    let mut command = Command::new(request.interpreter.executable());
    command
        .arg(&script_path)
        .current_dir(&dir)
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| DEFAULT_SAFE_PATH.into()),
        )
        .env("TMPDIR", &dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match SupervisedChild::spawn(&mut command) {
        Ok(child) => child,
        Err(err) => {
            result.launch_error = Some(format!(
                "lancement impossible : {err} (« {} » est-il installé sur ce worker ?)",
                request.interpreter.executable()
            ));
            return result;
        }
    };
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.take_stdin(), child.take_stdout(), child.take_stderr())
    else {
        result.launch_error = Some("tubes du script indisponibles".to_owned());
        return result;
    };

    let (mut stdout_bytes, mut stderr_bytes) = (Vec::new(), Vec::new());
    let started = Instant::now();
    let run = async {
        let _ = tokio::join!(
            read_bounded_into(stdout, &mut stdout_bytes),
            read_bounded_into(stderr, &mut stderr_bytes),
            send_stdin(Some(stdin), stdin_text.as_bytes()),
        );
        child.wait().await
    };
    match tokio::time::timeout(limit, run).await {
        Ok(Ok(status)) => {
            result.exit_code = status.code();
            child.finished();
        }
        Ok(Err(err)) => {
            result.launch_error = Some(format!("attente du script impossible : {err}"));
        }
        // Le groupe de processus est tué quand `child` est abandonné.
        Err(_) => result.timed_out = true,
    }
    result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    result.truncated =
        stdout_bytes.len() > TRIAL_CAPTURE_BYTES || stderr_bytes.len() > TRIAL_CAPTURE_BYTES;
    result.stdout = captured(&stdout_bytes);
    result.stderr = captured(&stderr_bytes);
    if !result.timed_out {
        match parse_script_response(&result.stdout) {
            Ok(response) => result.response = response,
            Err(message) => result.response_error = Some(message),
        }
    }
    result
}

/// Écrit le script et le fichier d'exemple ; renvoie le chemin du script et l'entrée JSON.
fn prepare(request: &ScriptTrialRequest, dir: &Path) -> Result<(PathBuf, String), String> {
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("dossier de travail impossible à créer : {err}"))?;
    let fallback = format!("run.{}", request.interpreter.default_extension());
    let script_path = dir.join(sanitize_script_filename(
        Some(request.entrypoint.as_deref().unwrap_or(&fallback)),
        0,
    ));
    std::fs::write(&script_path, &request.code)
        .map_err(|err| format!("écriture du script impossible : {err}"))?;
    let sample = dir.join(SAMPLE_FILE_NAME);
    std::fs::write(&sample, b"")
        .map_err(|err| format!("fichier d'exemple impossible à créer : {err}"))?;
    let sample = sample.to_string_lossy().into_owned();
    let media_info = Value::Object(serde_json::Map::new());
    let payload = ScriptInput {
        schema_version: SCRIPT_SCHEMA_VERSION,
        flow_id: TRIAL_IDENTIFIER,
        node_id: TRIAL_IDENTIFIER,
        operation_id: TRIAL_IDENTIFIER,
        working_file: sample.clone(),
        source_file: sample,
        work_dir: dir.to_string_lossy().into_owned(),
        media_info: &media_info,
        config: Value::Object(request.config.clone()),
        variables: json!({"file": {}, "job": {}, "vars": request.vars}),
    };
    let stdin_text = serde_json::to_string_pretty(&payload)
        .map_err(|err| format!("sérialisation de l'entrée impossible : {err}"))?;
    Ok((script_path, stdin_text))
}

/// Texte conservé d'une sortie : borné, puis jetons et identifiants d'URL masqués.
fn captured(bytes: &[u8]) -> String {
    let kept = &bytes[..bytes.len().min(TRIAL_CAPTURE_BYTES)];
    mask_log_secrets(&String::from_utf8_lossy(kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::{local::ScriptInterpreter, script_trial::ScriptOutcomeStatus};

    const SKIPPING_SCRIPT: &str = "cat > /dev/null\necho '{\"status\":\"skip\"}'";

    fn request(code: &str) -> ScriptTrialRequest {
        ScriptTrialRequest {
            worker_id: Uuid::nil(),
            interpreter: ScriptInterpreter::Sh,
            code: code.to_owned(),
            entrypoint: None,
            config: serde_json::Map::new(),
            vars: serde_json::Map::new(),
        }
    }

    fn entries(root: &Path) -> anyhow::Result<usize> {
        Ok(std::fs::read_dir(root)?.count())
    }

    #[tokio::test]
    async fn a_script_reads_the_json_input_and_answers_on_stdout() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut asked = request(SKIPPING_SCRIPT);
        asked.config.insert("seuil".into(), json!(3));
        let result = run_script_trial(&asked, root.path()).await;
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert!(result.stdin.contains("\"workingFile\""), "{}", result.stdin);
        assert!(result.stdin.contains("\"seuil\": 3"), "{}", result.stdin);
        let response = result
            .response
            .ok_or_else(|| anyhow::anyhow!("réponse absente"))?;
        assert_eq!(response.status, ScriptOutcomeStatus::Skip);
        assert_eq!(entries(root.path())?, 0, "dossier d'essai non supprimé");
        Ok(())
    }

    #[tokio::test]
    async fn a_failing_script_reports_its_exit_code_and_stderr() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let result = run_script_trial(&request("echo oups >&2\nexit 3"), root.path()).await;
        assert_eq!(result.exit_code, Some(3));
        assert!(result.stderr.contains("oups"));
        assert!(result.response.is_none() && result.response_error.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn a_stdout_that_is_not_json_is_explained() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let result = run_script_trial(&request("echo bonjour"), root.path()).await;
        assert!(result.response.is_none());
        let reason = result.response_error.unwrap_or_default();
        assert!(reason.contains("invalide"), "{reason}");
        Ok(())
    }

    #[tokio::test]
    async fn a_slow_script_is_stopped_at_the_limit_and_keeps_its_partial_output()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let slow = request("echo debut\nsleep 30");
        let limit = Duration::from_millis(1500);
        let result = run_script_trial_within(&slow, root.path(), limit).await;
        assert!(result.timed_out);
        assert_eq!(result.exit_code, None);
        assert!(result.stdout.contains("debut"), "{result:?}");
        assert_eq!(entries(root.path())?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_large_output_is_cut_and_flagged() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let code = format!("head -c {} /dev/zero | tr '\\0' a", TRIAL_CAPTURE_BYTES * 2);
        let result = run_script_trial(&request(&code), root.path()).await;
        assert!(result.truncated);
        assert!(result.stdout.len() <= TRIAL_CAPTURE_BYTES);
        Ok(())
    }
}
