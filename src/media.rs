use anyhow::Context;
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt},
    process::Command,
};

const TOOL_TIMEOUT: Duration = Duration::from_secs(3600);
const MAX_IDENTIFICATION_BYTES: u64 = 8 * 1024 * 1024;

pub fn executable() -> String {
    std::env::var("MKVMERGE").unwrap_or_else(|_| "mkvmerge".into())
}

pub fn propedit_executable() -> String {
    std::env::var("MKVPROPEDIT").unwrap_or_else(|_| "mkvpropedit".into())
}

/// Édition non destructive en place des en-têtes EBML Matroska (ADR 0008).
pub async fn propedit_in_place(
    bin: &Path,
    target: &Path,
    default_audio_lang: Option<&str>,
    force_subtitles: bool,
) -> anyhow::Result<()> {
    let mut cmd = Command::new(bin);
    cmd.arg(target);

    if let Some(lang) = default_audio_lang {
        cmd.arg("--edit")
            .arg("track:a1")
            .arg("--set")
            .arg(format!("language={lang}"))
            .arg("--set")
            .arg("flag-default=1");
    }

    if force_subtitles {
        cmd.arg("--edit")
            .arg("track:s1")
            .arg("--set")
            .arg("flag-forced=1");
    }

    let status = tokio::time::timeout(Duration::from_secs(60), cmd.status())
        .await
        .context("mkvpropedit timed out")??;

    anyhow::ensure!(
        matches!(status.code(), Some(0 | 1)),
        "mkvpropedit failed with code: {:?}",
        status.code()
    );
    Ok(())
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
    remux_with_progress(bin, source, output, |_, _| {}).await
}

pub async fn remux_with_progress<F>(
    bin: &Path,
    source: &Path,
    output: &Path,
    mut on_progress: F,
) -> anyhow::Result<bool>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    inspect_with(bin, source).await?;
    let mut child = Command::new(bin)
        .arg("--output-charset")
        .arg("UTF-8")
        .arg("--disable-track-statistics-tags")
        .arg("--output")
        .arg(output)
        .arg(source)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child.stdout.take().context("missing stdout")?;
    let mut reader = tokio::io::BufReader::new(stdout).lines();

    let read_progress = async {
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(pct_str) = line.strip_prefix("Progress: ") {
                let trimmed = pct_str.trim().trim_end_matches('%');
                if let Ok(pct) = trimmed.parse::<u32>() {
                    on_progress(pct, &line);
                }
            } else if line.contains('%') {
                on_progress(0, &line);
            }
        }
    };

    let wait_child = async {
        let status = child.wait().await?;
        anyhow::Ok(status)
    };

    let (status, _) = tokio::time::timeout(TOOL_TIMEOUT, async {
        tokio::try_join!(wait_child, async {
            read_progress.await;
            anyhow::Ok(())
        })
    })
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
