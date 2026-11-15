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
/// Taille maximale de l'extrait de console conservé par invocation (queue de stderr).
const MAX_CAPTURED_LOG_BYTES: usize = 4096;
const PERCENT_MAX: u32 = 100;
const RUNNING_PERCENT_CAP: u32 = 99;
/// Un exécutable tout juste écrit (téléchargé ou généré) peut rester brièvement « occupé » : ces
/// retentatives courtes laissent le noyau libérer le fichier (`ETXTBSY`).
const SPAWN_BUSY_RETRIES: u32 = 5;
const SPAWN_BUSY_DELAY: Duration = Duration::from_millis(25);

/// Résultat d'une invocation réussie avec extrait console borné et masqué.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolRunOutcome {
    pub excerpt: Option<String>,
}

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
    pub fn parse_progress(
        self,
        segment: &str,
        duration_secs: Option<f64>,
    ) -> Option<(u32, String)> {
        match self {
            Self::Ffmpeg => ffmpeg_progress(segment, duration_secs),
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
        excerpt: Option<String>,
    },
    #[error("{tool} : {source}")]
    Wait {
        tool: &'static str,
        source: std::io::Error,
    },
}

/// Parse une estampille temporelle FFmpeg (`HH:MM:SS.xx`) en secondes.
pub fn parse_ffmpeg_timestamp(stamp: &str) -> Option<f64> {
    let mut parts = stamp.trim().split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !hours.is_finite() || !minutes.is_finite() || !seconds.is_finite()
    {
        return None;
    }
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    (total >= 0.0).then_some(total)
}

fn ffmpeg_progress(segment: &str, duration_secs: Option<f64>) -> Option<(u32, String)> {
    if let Some(time) = segment.split("time=").nth(1) {
        let stamp = time.split_whitespace().next().unwrap_or("");
        let percent = match (parse_ffmpeg_timestamp(stamp), duration_secs) {
            (Some(elapsed), Some(total)) if total > 0.0 => {
                ((elapsed / total) * f64::from(PERCENT_MAX))
                    .round()
                    .clamp(0.0, f64::from(RUNNING_PERCENT_CAP)) as u32
            }
            _ => 0,
        };
        Some((percent, stamp.to_owned()))
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

/// Masque les éventuels secrets dans les lignes console capturées (jetons Bearer, identifiants URL).
pub fn mask_log_secrets(input: &str) -> String {
    let mut masked = Vec::new();
    for line in input.lines() {
        let mut clean = line.to_owned();
        if let Some(idx) = clean.find("://")
            && let Some(at) = clean[idx + 3..].find('@')
        {
            let auth_start = idx + 3;
            let auth_end = auth_start + at;
            let userinfo = &clean[auth_start..auth_end];
            if !userinfo.contains(['/', ' ', '\t']) {
                clean.replace_range(auth_start..auth_end, "***");
            }
        }
        if let Some(pos) = clean.to_ascii_lowercase().find("bearer ") {
            let token_start = pos + "bearer ".len();
            let rest = &clean[token_start..];
            let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            clean.replace_range(token_start..token_start + token_end, "***");
        }
        masked.push(clean);
    }
    masked.join("\n")
}

#[derive(Default)]
struct TailBuffer {
    lines: Vec<String>,
    total_bytes: usize,
}

impl TailBuffer {
    fn push(&mut self, segment: &str) {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            return;
        }
        let line = mask_log_secrets(trimmed);
        self.total_bytes += line.len() + 1;
        self.lines.push(line);
        while self.total_bytes > MAX_CAPTURED_LOG_BYTES && !self.lines.is_empty() {
            let removed = self.lines.remove(0);
            self.total_bytes = self.total_bytes.saturating_sub(removed.len() + 1);
        }
    }

    fn finish(self) -> Option<String> {
        (!self.lines.is_empty()).then(|| self.lines.join("\n"))
    }
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

/// Lance le processus ; réessaie quelques fois seulement si l'exécutable est encore occupé.
async fn spawn_tool(bin: &Path, args: &[String]) -> std::io::Result<tokio::process::Child> {
    let mut attempt = 0;
    loop {
        let spawned = Command::new(bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        match spawned {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && attempt < SPAWN_BUSY_RETRIES =>
            {
                attempt += 1;
                tokio::time::sleep(SPAWN_BUSY_DELAY).await;
            }
            other => return other,
        }
    }
}

/// Lance `bin args…` ; la progression est lue sur stderr. Succès seulement si le code de sortie est 0.
pub async fn run_with_progress<F>(
    tool: ExternalTool,
    bin: &Path,
    args: &[String],
    on_progress: F,
) -> Result<ToolRunOutcome, ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    run_with_duration(tool, bin, args, None, on_progress).await
}

/// Lance `bin args…` en calculant la progression FFmpeg par rapport à `duration_secs` si connue.
pub async fn run_with_duration<F>(
    tool: ExternalTool,
    bin: &Path,
    args: &[String],
    duration_secs: Option<f64>,
    mut on_progress: F,
) -> Result<ToolRunOutcome, ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    let label = tool.label();
    let mut child = spawn_tool(bin, args)
        .await
        .map_err(|source| ToolRunError::Spawn {
            tool: label,
            source,
        })?;
    let stderr = child.stderr.take().ok_or(ToolRunError::Spawn {
        tool: label,
        source: std::io::Error::other("stderr indisponible"),
    })?;
    let mut tail = TailBuffer::default();
    let read_progress = async {
        for_each_segment(stderr, |segment| {
            tail.push(segment);
            if let Some((percent, text)) = tool.parse_progress(segment, duration_secs) {
                on_progress(percent, &text);
            }
        })
        .await;
        tail.finish()
    };
    let (status, excerpt) = tokio::time::timeout(TOOL_TIMEOUT, async {
        tokio::join!(child.wait(), read_progress)
    })
    .await
    .map_err(|_| ToolRunError::Timeout { tool: label })
    .and_then(|(status, excerpt)| {
        status
            .map(|status| (status, excerpt))
            .map_err(|source| ToolRunError::Wait {
                tool: label,
                source,
            })
    })?;
    if status.code() == Some(0) {
        Ok(ToolRunOutcome { excerpt })
    } else {
        Err(ToolRunError::Failed {
            tool: label,
            code: status.code(),
            excerpt,
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
    fn ffmpeg_progress_reads_time_then_frame_and_computes_percent() {
        assert_eq!(
            ffmpeg_progress("frame=1 fps=2 time=00:00:03.50 bitrate=1k", None),
            Some((0, "00:00:03.50".into()))
        );
        assert_eq!(
            ffmpeg_progress("frame=1 fps=2 time=00:00:30.00 bitrate=1k", Some(60.0)),
            Some((50, "00:00:30.00".into()))
        );
        assert_eq!(
            ffmpeg_progress("frame=1 fps=2 time=00:01:05.00 bitrate=1k", Some(60.0)),
            Some((99, "00:01:05.00".into()))
        );
        assert_eq!(
            ffmpeg_progress("frame= 12", Some(60.0)),
            Some((0, "frame= 12".into()))
        );
        assert_eq!(ffmpeg_progress("Stream #0", None), None);
    }

    #[test]
    fn mask_log_secrets_strips_tokens_and_url_credentials() {
        assert_eq!(
            mask_log_secrets("Authorization: Bearer super_secret_token"),
            "Authorization: Bearer ***"
        );
        assert_eq!(
            mask_log_secrets("fetch https://user:pass123@host.example/file"),
            "fetch https://***@host.example/file"
        );
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
        let failing = fake_tool(
            dir.path(),
            "failing",
            "printf 'Authorization: Bearer tok123\\nboom\\n' >&2; exit 3",
        )?;
        let error = run_with_progress(ExternalTool::Ffmpeg, &failing, &[], |_, _| {}).await;
        match error {
            Err(ToolRunError::Failed {
                tool: "ffmpeg",
                code: Some(3),
                excerpt,
            }) => {
                let text = excerpt.unwrap_or_default();
                assert!(!text.contains("tok123"));
                assert!(text.contains("boom"));
            }
            other => panic!("attendu Failed, reçu {other:?}"),
        }
        let missing = dir.path().join("absent");
        let error = run_with_progress(ExternalTool::Mp4Box, &missing, &[], |_, _| {}).await;
        assert!(matches!(error, Err(ToolRunError::Spawn { .. })));
        Ok(())
    }
}
