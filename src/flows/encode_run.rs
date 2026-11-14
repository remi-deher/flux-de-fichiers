//! Lancement d'un encodage FFmpeg, avec repli automatique GPU → CPU (une seule retentative).

use super::{
    FlowProgressCallback,
    encode::{EncodePlan, argv_for},
};
use crate::{media, tool_run::ToolRunError};
use std::path::Path;
use uuid::Uuid;

async fn encode_once(
    bin: &Path,
    plan: &EncodePlan,
    source: &Path,
    output: &Path,
    on_progress: &FlowProgressCallback,
) -> Result<(), ToolRunError> {
    let args = argv_for(plan, source, output);
    let on_progress = on_progress.clone();
    media::encode_with_progress(bin, &args, move |percent: u32, line: &str| {
        on_progress(percent, line);
    })
    .await
}

/// Encode `source` vers `output`. Si FFmpeg échoue avec un encodeur matériel, journalise
/// le repli puis retente une fois en logiciel ; tout autre échec est renvoyé tel quel.
pub async fn run_encode(
    bin: &Path,
    plan: &EncodePlan,
    source: &Path,
    output: &Path,
    on_progress: &FlowProgressCallback,
    job_id: Uuid,
) -> Result<(), ToolRunError> {
    let error = match encode_once(bin, plan, source, output, on_progress).await {
        Err(error @ ToolRunError::Failed { .. }) => error,
        other => return other,
    };
    let Some(cpu_plan) = plan.cpu_fallback() else {
        return Err(error);
    };
    tracing::warn!(job_id = %job_id, error = %error, "encodeur matériel en échec, repli sur le CPU");
    // Une sortie partielle ne doit pas survivre à la retentative.
    let _ = tokio::fs::remove_file(output).await;
    encode_once(bin, &cpu_plan, source, output, on_progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::encode::video_plan;
    use crate::tool_run::test_support::fake_tool;
    use serde_json::json;
    use std::sync::Arc;

    /// Faux FFmpeg : journalise ses arguments, échoue sur un encodeur `*_nvenc`, sinon crée la sortie.
    fn fake_ffmpeg(dir: &Path, log: &Path) -> std::io::Result<std::path::PathBuf> {
        let script = format!(
            r#"echo "$@" >> '{}'
for a; do last=$a; case "$a" in *_nvenc) exit 1;; esac; done
: > "$last""#,
            log.display()
        );
        fake_tool(dir, "ffmpeg", &script)
    }

    async fn run(encoder: &str, bin: &Path, output: &Path) -> Result<(), ToolRunError> {
        let plan = EncodePlan::Video(video_plan(&json!({"codec": "h265", "encoder": encoder})));
        let progress: FlowProgressCallback = Arc::new(|_, _| {});
        run_encode(
            bin,
            &plan,
            Path::new("in.mkv"),
            output,
            &progress,
            Uuid::new_v4(),
        )
        .await
    }

    fn attempts(log: &Path) -> Vec<String> {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[tokio::test]
    async fn failing_gpu_encoder_retries_once_on_cpu() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let log = dir.path().join("log");
        let bin = fake_ffmpeg(dir.path(), &log)?;
        let output = dir.path().join("out.mkv");
        run("nvidia", &bin, &output).await?;
        let attempts = attempts(&log);
        assert_eq!(attempts.len(), 2);
        assert!(attempts[0].contains("hevc_nvenc"));
        assert!(attempts[1].contains("libx265"));
        assert!(output.is_file());
        Ok(())
    }

    #[tokio::test]
    async fn cpu_failure_is_not_retried() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let log = dir.path().join("log");
        let bin = fake_tool(
            dir.path(),
            "ffmpeg",
            &format!("echo x >> '{}'; exit 4", log.display()),
        )?;
        let error = run("cpu", &bin, &dir.path().join("out.mkv")).await;
        assert!(matches!(
            error,
            Err(ToolRunError::Failed { code: Some(4), .. })
        ));
        assert_eq!(attempts(&log).len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn failing_fallback_reports_the_second_error() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let log = dir.path().join("log");
        let bin = fake_tool(
            dir.path(),
            "ffmpeg",
            &format!("echo x >> '{}'; exit 5", log.display()),
        )?;
        let error = run("nvidia", &bin, &dir.path().join("out.mkv")).await;
        assert!(matches!(
            error,
            Err(ToolRunError::Failed { code: Some(5), .. })
        ));
        assert_eq!(attempts(&log).len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn missing_binary_is_not_retried() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let error = run(
            "nvidia",
            &dir.path().join("absent"),
            &dir.path().join("o.mkv"),
        )
        .await;
        assert!(matches!(error, Err(ToolRunError::Spawn { .. })));
        Ok(())
    }
}
