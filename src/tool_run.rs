//! Exécution déclarative d'un outil externe avec progression, codes de sortie et extrait borné.

use crate::supervision::SupervisedChild;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, time::Duration};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(3600);
const READ_CHUNK_BYTES: usize = 4096;
/// Garde-fou : un segment de sortie plus long est tronqué (jamais conservé en entier).
const MAX_SEGMENT_BYTES: usize = 64 * 1024;
/// Taille maximale de stdout JSON/texte capturée pour les sondes déclaratives.
pub const MAX_STDOUT_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
/// Taille maximale de l'extrait de console conservé par invocation.
const MAX_CAPTURED_LOG_BYTES: usize = 4096;
const PERCENT_MAX: u32 = 100;
const RUNNING_PERCENT_CAP: u32 = 99;
/// Un exécutable tout juste écrit (téléchargé ou généré) peut rester brièvement « occupé » : ces
/// retentatives courtes laissent le noyau libérer le fichier (`ETXTBSY`).
const SPAWN_BUSY_RETRIES: u32 = 5;
const SPAWN_BUSY_DELAY: Duration = Duration::from_millis(25);

/// Analyseur de progression déclaré dans `runtime.console.progressFormat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressParser {
    #[default]
    None,
    ElapsedTime,
    #[serde(rename = "fraction-100")]
    Fraction100,
    PercentLine,
}

impl ProgressParser {
    /// Progression `(pourcentage, texte)` d'après un segment de sortie, si elle en porte une.
    pub fn parse_progress(
        self,
        segment: &str,
        duration_secs: Option<f64>,
    ) -> Option<(u32, String)> {
        match self {
            Self::None => None,
            Self::ElapsedTime => elapsed_time_progress(segment, duration_secs),
            Self::Fraction100 => fraction_100_progress(segment),
            Self::PercentLine => percent_line_progress(segment),
        }
    }
}

/// Flux observé pour la progression déclarée dans `runtime.console.progressStream`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressStream {
    Stdout,
    #[default]
    Stderr,
}

/// Paramètres d'exécution d'une commande externe issus d'un contrat d'opération.
#[derive(Clone, Debug)]
pub struct CommandSpec<'a> {
    pub tool_label: &'a str,
    pub bin: &'a Path,
    pub args: &'a [String],
    pub progress_parser: ProgressParser,
    pub progress_stream: ProgressStream,
    pub success_exit_codes: &'a [i32],
    pub warning_exit_codes: &'a [i32],
    pub duration_secs: Option<f64>,
    pub timeout: Duration,
    pub capture_stdout: bool,
    /// `PATH` explicite de la commande (outils du cache en tête) ; hérité sinon.
    pub path_env: Option<std::ffi::OsString>,
}

impl<'a> CommandSpec<'a> {
    pub fn new(tool_label: &'a str, bin: &'a Path, args: &'a [String]) -> Self {
        Self {
            tool_label,
            bin,
            args,
            progress_parser: ProgressParser::None,
            progress_stream: ProgressStream::Stderr,
            success_exit_codes: &[0],
            warning_exit_codes: &[],
            duration_secs: None,
            timeout: DEFAULT_TOOL_TIMEOUT,
            capture_stdout: false,
            path_env: None,
        }
    }
}

/// Résultat d'une invocation réussie avec extrait console borné et masqué.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolRunOutcome {
    pub excerpt: Option<String>,
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub warning: bool,
}

/// Échec d'exécution d'un outil externe.
#[derive(Debug, Error)]
pub enum ToolRunError {
    #[error("{tool} : lancement impossible ({source})")]
    Spawn {
        tool: String,
        source: std::io::Error,
    },
    #[error("{tool} : délai dépassé")]
    Timeout { tool: String },
    #[error("{tool} : échec (code {code:?})")]
    Failed {
        tool: String,
        code: Option<i32>,
        excerpt: Option<String>,
    },
    #[error("{tool} : {source}")]
    Wait {
        tool: String,
        source: std::io::Error,
    },
}

/// Parse une estampille temporelle (`HH:MM:SS.xx`) en secondes.
pub fn parse_hms_timestamp(stamp: &str) -> Option<f64> {
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

fn elapsed_time_progress(segment: &str, duration_secs: Option<f64>) -> Option<(u32, String)> {
    if let Some(time) = segment.split("time=").nth(1) {
        let stamp = time.split_whitespace().next().unwrap_or("");
        let percent = match (parse_hms_timestamp(stamp), duration_secs) {
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

/// Parse une progression fractionnelle sur 100, par ex. `(50/100)`.
fn fraction_100_progress(segment: &str) -> Option<(u32, String)> {
    let suffix = format!("/{PERCENT_MAX})");
    let end = segment.rfind(&suffix)?;
    let start = segment[..end].rfind('(')? + 1;
    let percent = segment[start..end].trim().parse::<u32>().ok()?;
    Some((percent.min(PERCENT_MAX), segment.trim().to_owned()))
}

/// Parse une progression en pourcentage en ligne (`Progress: 42%` ou `42%`).
fn percent_line_progress(segment: &str) -> Option<(u32, String)> {
    let trimmed = segment.trim();
    if let Some(pct_str) = trimmed.strip_prefix("Progress: ") {
        let raw = pct_str.trim().trim_end_matches('%');
        if let Ok(pct) = raw.parse::<u32>() {
            return Some((pct.min(PERCENT_MAX), trimmed.to_owned()));
        }
    }
    if trimmed.contains('%') {
        return Some((0, trimmed.to_owned()));
    }
    None
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
pub(crate) struct TailBuffer {
    lines: Vec<String>,
    total_bytes: usize,
}

impl TailBuffer {
    pub(crate) fn push(&mut self, segment: &str) {
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

    fn merge(&mut self, other: Self) {
        for line in other.lines {
            self.push(&line);
        }
    }

    pub(crate) fn finish(self) -> Option<String> {
        (!self.lines.is_empty()).then(|| self.lines.join("\n"))
    }
}

/// Découpe un flux d'octets en segments séparés par `\r` ou `\n`.
pub(crate) async fn for_each_segment<R, F>(mut reader: R, mut on_segment: F)
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

pub(crate) async fn read_bounded_bytes<R: AsyncRead + Unpin>(
    reader: R,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_STDOUT_CAPTURE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    Ok(bytes)
}

async fn spawn_tool(
    bin: &Path,
    args: &[String],
    pipe_stdout: bool,
    path_env: Option<&std::ffi::OsStr>,
) -> std::io::Result<SupervisedChild> {
    let mut attempt = 0;
    loop {
        let mut command = Command::new(bin);
        if let Some(path) = path_env {
            command.env("PATH", path);
        }
        command
            .args(args)
            .stdout(if pipe_stdout {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(Stdio::piped());
        match SupervisedChild::spawn(&mut command) {
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

/// Exécute une commande externe selon un contrat déclaratif (`CommandSpec`).
pub async fn run_command<F>(
    spec: CommandSpec<'_>,
    mut on_progress: F,
) -> Result<ToolRunOutcome, ToolRunError>
where
    F: FnMut(u32, &str) + Send + 'static,
{
    let pipe_stdout = spec.capture_stdout || spec.progress_stream == ProgressStream::Stdout;
    let mut child = spawn_tool(spec.bin, spec.args, pipe_stdout, spec.path_env.as_deref())
        .await
        .map_err(|source| ToolRunError::Spawn {
            tool: spec.tool_label.to_owned(),
            source,
        })?;
    let stderr = child.take_stderr().ok_or_else(|| ToolRunError::Spawn {
        tool: spec.tool_label.to_owned(),
        source: std::io::Error::other("stderr indisponible"),
    })?;
    let stdout = child.take_stdout();

    let (status, stdout_bytes, excerpt) = tokio::time::timeout(spec.timeout, async {
        let read_streams = async {
            let mut tail = TailBuffer::default();
            if spec.capture_stdout {
                let Some(out_reader) = stdout else {
                    for_each_segment(stderr, |segment| tail.push(segment)).await;
                    return (Vec::new(), tail.finish());
                };
                let (captured, ()) = tokio::join!(
                    read_bounded_bytes(out_reader),
                    for_each_segment(stderr, |segment| tail.push(segment)),
                );
                return (captured.unwrap_or_default(), tail.finish());
            }

            if spec.progress_stream == ProgressStream::Stdout
                && let Some(out_reader) = stdout
            {
                let mut stderr_tail = TailBuffer::default();
                tokio::join!(
                    for_each_segment(out_reader, |segment| {
                        tail.push(segment);
                        if let Some((pct, text)) = spec
                            .progress_parser
                            .parse_progress(segment, spec.duration_secs)
                        {
                            on_progress(pct, &text);
                        }
                    }),
                    for_each_segment(stderr, |segment| stderr_tail.push(segment)),
                );
                tail.merge(stderr_tail);
                return (Vec::new(), tail.finish());
            }

            for_each_segment(stderr, |segment| {
                tail.push(segment);
                if let Some((pct, text)) = spec
                    .progress_parser
                    .parse_progress(segment, spec.duration_secs)
                {
                    on_progress(pct, &text);
                }
            })
            .await;
            (Vec::new(), tail.finish())
        };

        let (wait_res, (stdout_bytes, excerpt)) = tokio::join!(child.wait(), read_streams);
        (wait_res, stdout_bytes, excerpt)
    })
    .await
    .map_err(|_| ToolRunError::Timeout {
        tool: spec.tool_label.to_owned(),
    })
    .and_then(|(status_res, stdout_bytes, excerpt)| {
        status_res
            .map(|status| (status, stdout_bytes, excerpt))
            .map_err(|source| ToolRunError::Wait {
                tool: spec.tool_label.to_owned(),
                source,
            })
    })?;
    child.finished();

    if spec.capture_stdout && stdout_bytes.len() > MAX_STDOUT_CAPTURE_BYTES {
        return Err(ToolRunError::Failed {
            tool: spec.tool_label.to_owned(),
            code: status.code(),
            excerpt: Some("sortie standard trop volumineuse".to_owned()),
        });
    }

    let success_codes = if spec.success_exit_codes.is_empty() {
        &[0][..]
    } else {
        spec.success_exit_codes
    };
    let exit_code = status.code().unwrap_or(-1);
    if success_codes.contains(&exit_code) {
        Ok(ToolRunOutcome {
            excerpt,
            exit_code,
            stdout: stdout_bytes,
            warning: spec.warning_exit_codes.contains(&exit_code),
        })
    } else {
        Err(ToolRunError::Failed {
            tool: spec.tool_label.to_owned(),
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
    fn elapsed_time_progress_reads_time_then_frame_and_computes_percent() {
        assert_eq!(
            elapsed_time_progress("frame=1 fps=2 time=00:00:03.50 bitrate=1k", None),
            Some((0, "00:00:03.50".into()))
        );
        assert_eq!(
            elapsed_time_progress("frame=1 fps=2 time=00:00:30.00 bitrate=1k", Some(60.0)),
            Some((50, "00:00:30.00".into()))
        );
        assert_eq!(
            elapsed_time_progress("frame=1 fps=2 time=00:01:05.00 bitrate=1k", Some(60.0)),
            Some((99, "00:01:05.00".into()))
        );
        assert_eq!(
            elapsed_time_progress("frame= 12", Some(60.0)),
            Some((0, "frame= 12".into()))
        );
        assert_eq!(elapsed_time_progress("Stream #0", None), None);
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
    fn fraction_and_percent_parsers_extract_percentages() {
        let line = "ISO File Writing: |=====     | (50/100)";
        assert_eq!(fraction_100_progress(line), Some((50, line.into())));
        assert_eq!(
            fraction_100_progress("Importing x (250/100)").map(|p| p.0),
            Some(100)
        );
        assert_eq!(fraction_100_progress("(abc/100)"), None);
        assert_eq!(
            percent_line_progress("Progress: 75%"),
            Some((75, "Progress: 75%".into()))
        );
    }

    #[tokio::test]
    async fn segments_split_on_carriage_returns_and_newlines() {
        let mut seen = Vec::new();
        for_each_segment(&b"a\rb\r\nc"[..], |segment| seen.push(segment.to_owned())).await;
        assert_eq!(seen, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn runs_command_spec_progress_and_captures_stdout()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let bin = fake_tool(
            dir.path(),
            "fake",
            r#"printf 'x (10/100)\r(100/100)\r' >&2"#,
        )?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let mut spec = CommandSpec::new("fake", &bin, &[]);
        spec.progress_parser = ProgressParser::Fraction100;
        run_command(spec, move |percent, _| {
            if let Ok(mut seen) = sink.lock() {
                seen.push(percent);
            }
        })
        .await?;
        assert_eq!(*seen.lock().map_err(|e| e.to_string())?, [10, 100]);

        let json_bin = fake_tool(dir.path(), "probe", r#"printf '{"ok":true}'"#)?;
        let mut probe_spec = CommandSpec::new("probe", &json_bin, &[]);
        probe_spec.capture_stdout = true;
        probe_spec.success_exit_codes = &[0, 1];
        probe_spec.warning_exit_codes = &[1];
        let out = run_command(probe_spec, |_, _| {}).await?;
        assert_eq!(out.stdout, b"{\"ok\":true}");
        assert!(!out.warning);
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
        let spec = CommandSpec::new("ffmpeg", &failing, &[]);
        let error = run_command(spec, |_, _| {}).await;
        match error {
            Err(ToolRunError::Failed {
                tool,
                code: Some(3),
                excerpt,
            }) => {
                assert_eq!(tool, "ffmpeg");
                let text = excerpt.unwrap_or_default();
                assert!(!text.contains("tok123"));
                assert!(text.contains("boom"));
            }
            other => panic!("attendu Failed, reçu {other:?}"),
        }
        let missing = dir.path().join("absent");
        let missing_spec = CommandSpec::new("mp4box", &missing, &[]);
        let error = run_command(missing_spec, |_, _| {}).await;
        assert!(matches!(error, Err(ToolRunError::Spawn { .. })));
        Ok(())
    }
}
