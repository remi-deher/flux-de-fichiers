use anyhow::Context;
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

const TOOL_TIMEOUT: Duration = Duration::from_secs(3600);
const MAX_IDENTIFICATION_BYTES: u64 = 8 * 1024 * 1024;

pub fn executable() -> String {
    std::env::var("MKVMERGE").unwrap_or_else(|_| "mkvmerge".into())
}

pub async fn inspect(path: &Path) -> anyhow::Result<Value> {
    inspect_with(Path::new(&executable()), path).await
}

pub async fn inspect_with(bin: &Path, path: &Path) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut child = Command::new(bin)
            .arg("--output-charset")
            .arg("UTF-8")
            .arg("-J")
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .context("missing stdout")?
            .take(MAX_IDENTIFICATION_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_IDENTIFICATION_BYTES,
            "identification too large"
        );
        anyhow::ensure!(
            matches!(child.wait().await?.code(), Some(0 | 1)),
            "identification failed"
        );
        let data: Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            data["container"]["recognized"] == true && data["container"]["supported"] == true,
            "unsupported container"
        );
        anyhow::ensure!(
            data["tracks"].as_array().is_some_and(|t| !t.is_empty()),
            "no tracks"
        );
        Ok(data)
    })
    .await
    .context("identification timed out")?
}

fn track_signature(data: &Value) -> Vec<Value> {
    data["tracks"]
        .as_array()
        .map(|tracks| {
            tracks
                .iter()
                .map(|t| {
                    let p = &t["properties"];
                    serde_json::json!([
                        t["type"],
                        t["codec"],
                        p["language"],
                        p["enabled_track"],
                        p["track_name"],
                        p["default_track"],
                        p["forced_track"],
                        p["audio_channels"],
                        p["audio_sampling_frequency"],
                        p["pixel_dimensions"]
                    ])
                })
                .collect()
        })
        .unwrap_or_default()
}

pub async fn validate_remux(source: &Path, output: &Path) -> anyhow::Result<()> {
    validate_remux_with(Path::new(&executable()), source, output).await
}

pub async fn validate_remux_with(bin: &Path, source: &Path, output: &Path) -> anyhow::Result<()> {
    let before = inspect_with(bin, source).await?;
    let after = inspect_with(bin, output).await?;
    anyhow::ensure!(
        track_signature(&before) == track_signature(&after),
        "track metadata mismatch"
    );
    anyhow::ensure!(
        before["container"]["properties"]["title"] == after["container"]["properties"]["title"],
        "container title mismatch"
    );
    let attachment_signature = |v: &Value| -> Vec<Value> {
        v["attachments"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|a| serde_json::json!([a["file_name"], a["content_type"], a["size"]]))
                    .collect()
            })
            .unwrap_or_default()
    };
    anyhow::ensure!(
        attachment_signature(&before) == attachment_signature(&after),
        "attachment mismatch"
    );
    anyhow::ensure!(
        before["chapters"] == after["chapters"]
            && before["global_tags"] == after["global_tags"]
            && before["track_tags"] == after["track_tags"],
        "chapter or tag mismatch"
    );
    Ok(())
}

/// MKVToolNix: 0 = success, 1 = success with warnings, 2 = error.
pub async fn remux(source: &Path, output: &Path) -> anyhow::Result<bool> {
    remux_with(Path::new(&executable()), source, output).await
}

pub async fn remux_with(bin: &Path, source: &Path, output: &Path) -> anyhow::Result<bool> {
    inspect_with(bin, source).await?;
    let status = tokio::time::timeout(
        TOOL_TIMEOUT,
        Command::new(bin)
            .arg("--output-charset")
            .arg("UTF-8")
            .arg("--disable-track-statistics-tags")
            .arg("--output")
            .arg(output)
            .arg(source)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .context("remux timed out")??;
    tracing::info!(exit_code = ?status.code(), "mkvmerge finished");
    let warning = match status.code() {
        Some(0) => false,
        Some(1) => true,
        _ => anyhow::bail!("mkvmerge failed"),
    };
    validate_remux_with(bin, source, output).await?;
    Ok(warning)
}
