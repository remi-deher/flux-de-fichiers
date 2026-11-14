//! Exécution d'un outil externe (FFmpeg, MP4Box) avec progression lue sur sa sortie d'erreur.

use std::{path::Path, process::Stdio, time::Duration};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

const TOOL_TIMEOUT: Duration = Duration::from_secs(3600);
const READ_CHUNK_BYTES: usize = 4096;
/// Garde-fou : un segment de sortie plus long est tronqué (jamais conservé en entier).
const MAX_SEGMENT_BYTES: usize = 64 * 1024;
const PERCENT_MAX: u32 = 100;

/// Outil externe piloté par le moteur de flux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalTool {
    Ffmpeg,
    Mp4Box,
}

impl ExternalTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::Mp4Box => "mp4box",
        }
    }

    /// Progression `(pourcentage, texte)` d'après un segment de sortie, si elle en porte une.
    pub fn parse_progress(self, segment: &str) -> Option<(u32, String)> {
        match self {
            Self::Ffmpeg => ffmpeg_progress(segment),
            Self::Mp4Box => mp4box_progress(segment),
        }
    }
}

/// Échec d'exécution d'un outil externe.
#[derive(Debug, Error)]
pub enum ToolRunError {
    #[error("{tool} : lancement impossible ({source})")]
    Spawn {
        tool: &'static str,
        source: std::io::Error,
    },
    #[error("{tool} : délai dépassé")]
    Timeout { tool: &'static str },
    #[error("{tool} : échec (code {code:?})")]
    Failed {
        tool: &'static str,
        code: Option<i32>,
    },
    #[error("{tool} : {source}")]
    Wait {
        tool: &'static str,
        source: std::io::Error,
    },
}

fn ffmpeg_progress(segment: &str) -> Option<(u32, String)> {
    if let Some(time) = segment.split("time=").nth(1) {
        let stamp = time.split_whitespace().next().unwrap_or("");
        Some((0, stamp.to_owned()))
    } else if segment.contains("frame=") {
        Some((0, segment.to_owned()))
    } else {
        None
    }
}

/// MP4Box écrit `ISO File Writing: |=====   | (50/100)` ; on lit le dernier `(n/100)`.
fn mp4box_progress(segment: &str) -> Option<(u32, String)> {
    let suffix = format!("/{PERCENT_MAX})");
    let end = segment.rfind(&suffix)?;
    let start = segment[..end].rfind('(')? + 1;
    let percent = segment[start..end].trim().parse::<u32>().ok()?;
    Some((percent.min(PERCENT_MAX), segment.trim().to_owned()))
}

/// Découpe un flux d'octets en segments séparés par `\r` ou `\n` (les outils réécrivent
/// leur ligne de progression avec `\r`).
async fn for_each_segment<R, F>(mut reader: R, mut on_segment: F)
where
    R: AsyncRead + Unpin,
    F: FnMut(&str),
{
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    let mut pending: Vec<u8> = Vec::new();
    let mut flush = |pending: &mut Vec<u8>| {
        if !pending.is_empty() {
            on_segment(&String::from_utf8_lossy(pending));
            pending.clear();
        }
    };
    while let Ok(read) = reader.read(&mut chunk).await {
        if read == 0 {
            break;
        }
        for &byte in &chunk[..read] {
            if byte == b'\r' || byte == b'\n' {
                flush(&mut pending);
            } else if pending.len() < MAX_SEGMENT_BYTES {
                pending.push(byte);
            }
        }
    }
    flush(&mut pending);
}

/// Lance `bin args…` ; la progression est lue sur stderr. Succès seulement si le code de sortie est 0.
pub async fn run_with_progress<F>(
    tool: ExternalTool,
    bin: &Path,
    args: &[String],
    mut on_progress: F,
) -> Result<(), ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    let label = tool.label();
    let mut child = Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| ToolRunError::Spawn {
            tool: label,
            source,
        })?;
    let stderr = child.stderr.take().ok_or(ToolRunError::Spawn {
        tool: label,
        source: std::io::Error::other("stderr indisponible"),
    })?;
    let read_progress = for_each_segment(stderr, |segment| {
        if let Some((percent, text)) = tool.parse_progress(segment) {
            on_progress(percent, &text);
        }
    });
    let (status, ()) = tokio::time::timeout(TOOL_TIMEOUT, async {
        tokio::join!(child.wait(), read_progress)
    })
    .await
    .map_err(|_| ToolRunError::Timeout { tool: label })
    .and_then(|(status, ())| {
        status
            .map(|status| (status, ()))
            .map_err(|source| ToolRunError::Wait {
                tool: label,
                source,
            })
    })?;
    if status.code() == Some(0) {
        Ok(())
    } else {
        Err(ToolRunError::Failed {
            tool: label,
            code: status.code(),
        })
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    /// Écrit un faux exécutable shell dans `dir` et renvoie son chemin.
    pub fn fake_tool(dir: &std::path::Path, name: &str, script: &str) -> std::io::Result<PathBuf> {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::fake_tool;
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn ffmpeg_progress_reads_time_then_frame() {
        assert_eq!(
            ffmpeg_progress("frame=1 fps=2 time=00:00:03.50 bitrate=1k"),
            Some((0, "00:00:03.50".into()))
        );
        assert_eq!(ffmpeg_progress("frame= 12"), Some((0, "frame= 12".into())));
        assert_eq!(ffmpeg_progress("Stream #0"), None);
    }

    #[test]
    fn mp4box_progress_reads_the_last_percentage() {
        let line = "ISO File Writing: |=====     | (50/100)";
        assert_eq!(mp4box_progress(line), Some((50, line.into())));
        assert_eq!(
            mp4box_progress("Importing x (250/100)").map(|p| p.0),
            Some(100)
        );
        assert_eq!(mp4box_progress("(abc/100)"), None);
        assert_eq!(mp4box_progress("no progress"), None);
    }

    #[tokio::test]
    async fn segments_split_on_carriage_returns_and_newlines() {
        let mut seen = Vec::new();
        for_each_segment(&b"a\rb\r\nc"[..], |segment| seen.push(segment.to_owned())).await;
        assert_eq!(seen, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn runs_a_fake_tool_and_reports_progress() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let bin = fake_tool(
            dir.path(),
            "fake",
            r#"printf 'x (10/100)\r(100/100)\r' >&2"#,
        )?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        run_with_progress(ExternalTool::Mp4Box, &bin, &[], move |percent, _| {
            if let Ok(mut seen) = sink.lock() {
                seen.push(percent);
            }
        })
        .await?;
        assert_eq!(*seen.lock().map_err(|e| e.to_string())?, [10, 100]);
        Ok(())
    }

    #[tokio::test]
    async fn typed_errors_for_failure_and_missing_binary() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let failing = fake_tool(dir.path(), "failing", "exit 3")?;
        let error = run_with_progress(ExternalTool::Ffmpeg, &failing, &[], |_, _| {}).await;
        assert!(matches!(
            error,
            Err(ToolRunError::Failed {
                tool: "ffmpeg",
                code: Some(3)
            })
        ));
        let missing = dir.path().join("absent");
        let error = run_with_progress(ExternalTool::Mp4Box, &missing, &[], |_, _| {}).await;
        assert!(matches!(error, Err(ToolRunError::Spawn { .. })));
        Ok(())
    }
}
