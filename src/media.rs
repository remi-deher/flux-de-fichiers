use crate::tool_run::{
    ExternalTool, ToolRunError, ToolRunOutcome, run_with_duration, run_with_progress,
};
use anyhow::Context;
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt},
    process::Command,
};

const TOOL_TIMEOUT: Duration = Duration::from_secs(3600);
const MAX_IDENTIFICATION_BYTES: u64 = 8 * 1024 * 1024;
const NANOSECONDS_PER_SECOND: f64 = 1_000_000_000.0;
const MP4_MIN_HEADER_BYTES: usize = 8;

/// Profil de validation d'un fichier de sortie selon les opérations du flux exécuté.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputProfile {
    /// Remuxage Matroska sans perte : pistes, chapitres, tags et pièces jointes identiques.
    StrictMkvRemux,
    /// Édition d'en-têtes in-place (`mkvpropedit`) : même nombre et codecs de pistes, flags/langues modifiables.
    MkvMetadataEdit,
    /// Transcodage vidéo ou audio vers MKV : conteneur Matroska valide avec pistes présentes.
    TranscodedMkv,
    /// Remuxage vers MP4 (`MP4Box`) : fichier non vide portant une boîte ISO BMFF `ftyp`.
    Mp4Container,
}

impl OutputProfile {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp4Container => "mp4",
            Self::StrictMkvRemux | Self::MkvMetadataEdit | Self::TranscodedMkv => "mkv",
        }
    }

    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Mp4Container => "video/mp4",
            Self::StrictMkvRemux | Self::MkvMetadataEdit | Self::TranscodedMkv => {
                "video/x-matroska"
            }
        }
    }
}

pub fn executable() -> String {
    std::env::var("MKVMERGE").unwrap_or_else(|_| "mkvmerge".into())
}

pub fn propedit_executable() -> String {
    std::env::var("MKVPROPEDIT").unwrap_or_else(|_| "mkvpropedit".into())
}

pub fn ffmpeg_executable() -> String {
    std::env::var("FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
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

/// Extrait la durée totale en secondes depuis `mkvmerge -J` ou MediaInfo JSON, si disponible.
pub fn media_duration_secs(info: &Value) -> Option<f64> {
    if let Some(ns) = info
        .pointer("/container/properties/duration")
        .and_then(Value::as_u64)
        && ns > 0
    {
        return Some(ns as f64 / NANOSECONDS_PER_SECOND);
    }
    if let Some(tracks) = info
        .pointer("/mediainfo/media/track")
        .and_then(Value::as_array)
    {
        for track in tracks {
            if let Some(raw) = track.get("Duration").and_then(Value::as_str)
                && let Ok(secs) = raw.parse::<f64>()
                && secs > 0.0
            {
                return Some(secs);
            }
            if let Some(secs) = track.get("Duration").and_then(Value::as_f64)
                && secs > 0.0
            {
                return Some(secs);
            }
        }
    }
    None
}

/// Interroge `mediainfo --Output=JSON` pour enrichir l'arbre technique du flux.
pub async fn inspect_mediainfo(bin: &Path, path: &Path) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut child = Command::new(bin)
            .arg("--Output=JSON")
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .context("missing mediainfo stdout")?
            .take(MAX_IDENTIFICATION_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_IDENTIFICATION_BYTES,
            "mediainfo output too large"
        );
        let status = child.wait().await?;
        anyhow::ensure!(status.success(), "mediainfo failed");
        let parsed: Value = serde_json::from_slice(&bytes)?;
        Ok(parsed)
    })
    .await
    .context("mediainfo timed out")?
}

/// Fusionne la sortie `mediainfo --Output=JSON` dans l'arbre `mkvmerge -J` sous `/mediainfo`.
pub fn attach_mediainfo(mut mkv_info: Value, mediainfo: Value) -> Value {
    if let Some(object) = mkv_info.as_object_mut() {
        object.insert("mediainfo".to_owned(), mediainfo);
    }
    mkv_info
}

fn track_codecs(data: &Value) -> Vec<(Option<&str>, Option<&str>)> {
    data["tracks"]
        .as_array()
        .map(|tracks| {
            tracks
                .iter()
                .map(|t| (t["type"].as_str(), t["codec"].as_str()))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn validate_output(
    profile: OutputProfile,
    source: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    validate_output_with(Path::new(&executable()), profile, source, output).await
}

/// Valide la sortie produite selon le profil attendu par le flux.
pub async fn validate_output_with(
    mkvmerge_bin: &Path,
    profile: OutputProfile,
    source: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    match profile {
        OutputProfile::StrictMkvRemux => validate_remux_with(mkvmerge_bin, source, output).await,
        OutputProfile::MkvMetadataEdit => {
            let before = inspect_with(mkvmerge_bin, source).await?;
            let after = inspect_with(mkvmerge_bin, output).await?;
            anyhow::ensure!(
                track_codecs(&before) == track_codecs(&after),
                "track layout mismatch after metadata edit"
            );
            Ok(())
        }
        OutputProfile::TranscodedMkv => {
            let meta = tokio::fs::metadata(output).await?;
            anyhow::ensure!(meta.len() > 0, "empty transcoded output");
            let _ = inspect_with(mkvmerge_bin, output).await?;
            Ok(())
        }
        OutputProfile::Mp4Container => validate_mp4_header(output).await,
    }
}

async fn validate_mp4_header(output: &Path) -> anyhow::Result<()> {
    let mut file = tokio::fs::File::open(output).await?;
    let size = file.metadata().await?.len();
    anyhow::ensure!(size >= MP4_MIN_HEADER_BYTES as u64, "mp4 output too small");
    let mut header = [0u8; 16];
    let read = file.read(&mut header).await?;
    anyhow::ensure!(read >= MP4_MIN_HEADER_BYTES, "incomplete mp4 header");
    anyhow::ensure!(&header[4..8] == b"ftyp", "missing ISO BMFF ftyp box");
    Ok(())
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

/// Lance FFmpeg avec un argv déjà construit (sans l'exécutable) ; progresse via stderr.
pub async fn encode_with_progress<F>(
    bin: &Path,
    args: &[String],
    on_progress: F,
) -> Result<ToolRunOutcome, ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    run_with_progress(ExternalTool::Ffmpeg, bin, args, on_progress).await
}

/// Lance FFmpeg avec calcul de pourcentage basé sur `duration_secs` si connue.
pub async fn encode_with_duration<F>(
    bin: &Path,
    args: &[String],
    duration_secs: Option<f64>,
    on_progress: F,
) -> Result<ToolRunOutcome, ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    run_with_duration(ExternalTool::Ffmpeg, bin, args, duration_secs, on_progress).await
}

/// Lance MP4Box avec un argv déjà construit (sans l'exécutable) ; progresse via stderr.
pub async fn mp4box_with_progress<F>(
    bin: &Path,
    args: &[String],
    on_progress: F,
) -> Result<ToolRunOutcome, ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    run_with_progress(ExternalTool::Mp4Box, bin, args, on_progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_run::test_support::fake_tool;

    #[test]
    fn media_duration_reads_mkvmerge_nanoseconds_or_mediainfo_seconds() {
        let mkv = serde_json::json!({
            "container": {"properties": {"duration": 120_500_000_000u64}}
        });
        assert_eq!(media_duration_secs(&mkv), Some(120.5));

        let mi = serde_json::json!({
            "mediainfo": {"media": {"track": [{"@type": "General", "Duration": "95.250"}]}}
        });
        assert_eq!(media_duration_secs(&mi), Some(95.25));
    }

    #[tokio::test]
    async fn validate_mp4_header_accepts_ftyp_and_rejects_garbage()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let valid = dir.path().join("ok.mp4");
        tokio::fs::write(&valid, b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00").await?;
        validate_output(OutputProfile::Mp4Container, &valid, &valid).await?;

        let invalid = dir.path().join("bad.mp4");
        tokio::fs::write(&invalid, b"not-an-mp4-file").await?;
        assert!(
            validate_output(OutputProfile::Mp4Container, &valid, &invalid)
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn validate_output_with_distinguishes_metadata_edit_and_transcode()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("src.mkv");
        let dst = dir.path().join("dst.mkv");
        tokio::fs::write(&src, b"src").await?;
        tokio::fs::write(&dst, b"dst").await?;

        let bin = fake_tool(
            dir.path(),
            "mkvmerge",
            r#"for a; do last=$a; done
case "$last" in
  *src.mkv) echo '{"container":{"recognized":true,"supported":true},"tracks":[{"type":"video","codec":"AVC","properties":{"language":"und"}}]}';;
  *dst.mkv) echo '{"container":{"recognized":true,"supported":true},"tracks":[{"type":"video","codec":"AVC","properties":{"language":"fre"}}]}';;
esac"#,
        )?;
        assert!(
            validate_output_with(&bin, OutputProfile::StrictMkvRemux, &src, &dst)
                .await
                .is_err()
        );
        validate_output_with(&bin, OutputProfile::MkvMetadataEdit, &src, &dst).await?;
        validate_output_with(&bin, OutputProfile::TranscodedMkv, &src, &dst).await?;
        Ok(())
    }
}
