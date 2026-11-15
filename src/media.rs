use crate::tool_run::{CommandSpec, ProgressParser, ProgressStream, run_command};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, time::Duration};
use tokio::io::AsyncReadExt;

const NANOSECONDS_PER_SECOND: f64 = 1_000_000_000.0;
const MP4_MIN_HEADER_BYTES: usize = 8;
const PROBE_DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Profil de validation et d'extension de sortie dérivé du contrat d'opération final.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputProfile {
    /// Remuxage Matroska sans perte : pistes, chapitres, tags et pièces jointes identiques.
    StrictMkvRemux,
    /// Édition d'en-têtes in-place : même nombre et codecs de pistes, flags/langues modifiables.
    MkvMetadataEdit,
    /// Transcodage vidéo ou audio vers MKV : conteneur Matroska valide avec pistes présentes.
    TranscodedMkv,
    /// Remuxage vers MP4 : fichier non vide portant une boîte ISO BMFF `ftyp`.
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

    pub const fn post_validate(self) -> PostValidateMode {
        match self {
            Self::StrictMkvRemux => PostValidateMode::RemuxEquivalent,
            Self::MkvMetadataEdit => PostValidateMode::TracksUnchanged,
            Self::TranscodedMkv => PostValidateMode::ValidMedia,
            Self::Mp4Container => PostValidateMode::Mp4Header,
        }
    }
}

/// Mode de validation post-exécution déclaré par un contrat d'opération (`runtime.postValidate`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PostValidateMode {
    #[default]
    None,
    RemuxEquivalent,
    TracksUnchanged,
    ValidMedia,
    Mp4Header,
}

/// Format JSON attendu d'une sonde déclarée (`runtime.probeFormat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeFormat {
    #[default]
    MkvmergeJson,
    MediainfoJson,
    RawJson,
}

/// Paramètres nécessaires pour exécuter une sonde binaire déclarative sur un chemin média.
#[derive(Clone, Debug)]
pub struct ProbeCommand<'a> {
    pub label: &'a str,
    pub bin: &'a Path,
    pub args: &'a [String],
    pub format: ProbeFormat,
    pub success_exit_codes: &'a [i32],
    pub timeout: Duration,
}

impl<'a> ProbeCommand<'a> {
    pub fn new(label: &'a str, bin: &'a Path, args: &'a [String]) -> Self {
        Self {
            label,
            bin,
            args,
            format: ProbeFormat::MkvmergeJson,
            success_exit_codes: &[0, 1],
            timeout: Duration::from_secs(PROBE_DEFAULT_TIMEOUT_SECS),
        }
    }
}

/// Exécute une sonde binaire et valide la structure JSON selon son `ProbeFormat`.
pub async fn run_probe_json(cmd: ProbeCommand<'_>) -> anyhow::Result<Value> {
    let mut spec = CommandSpec::new(cmd.label, cmd.bin, cmd.args);
    spec.progress_parser = ProgressParser::None;
    spec.progress_stream = ProgressStream::Stderr;
    spec.success_exit_codes = cmd.success_exit_codes;
    spec.timeout = cmd.timeout;
    spec.capture_stdout = true;

    let outcome = run_command(spec, |_, _| {}).await?;
    let data: Value = serde_json::from_slice(&outcome.stdout)?;
    validate_probe_json(cmd.format, &data)?;
    Ok(data)
}

/// Valide la structure JSON minimale retournée par une sonde selon son format déclaré.
pub fn validate_probe_json(format: ProbeFormat, data: &Value) -> anyhow::Result<()> {
    match format {
        ProbeFormat::MkvmergeJson => {
            anyhow::ensure!(
                data["container"]["recognized"] == true && data["container"]["supported"] == true,
                "unsupported container"
            );
            anyhow::ensure!(
                data["tracks"].as_array().is_some_and(|t| !t.is_empty()),
                "no tracks"
            );
        }
        ProbeFormat::MediainfoJson => {
            anyhow::ensure!(data.get("media").is_some(), "missing mediainfo media node");
        }
        ProbeFormat::RawJson => {
            anyhow::ensure!(data.is_object() || data.is_array(), "invalid probe json");
        }
    }
    Ok(())
}

/// Fusionne la sortie d'une sonde d'enrichissement dans l'arbre technique sous `key`.
pub fn merge_probe_enrichment(mut primary: Value, key: &str, enrichment: Value) -> Value {
    if let Some(object) = primary.as_object_mut() {
        object.insert(key.to_owned(), enrichment);
    }
    primary
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

/// Extrait la durée totale en secondes depuis un arbre de sonde MKV ou MediaInfo.
pub fn media_duration_secs(info: &Value) -> Option<f64> {
    if let Some(ns) = info
        .pointer("/container/properties/duration")
        .and_then(Value::as_u64)
        && ns > 0
    {
        return Some(ns as f64 / NANOSECONDS_PER_SECOND);
    }
    for pointer in ["/mediainfo/media/track", "/media/track"] {
        if let Some(tracks) = info.pointer(pointer).and_then(Value::as_array) {
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
    }
    None
}

/// Valide qu'un fichier MP4 commence par une boîte ISO BMFF `ftyp`.
pub async fn validate_mp4_header(output: &Path) -> anyhow::Result<()> {
    let mut file = tokio::fs::File::open(output).await?;
    let size = file.metadata().await?.len();
    anyhow::ensure!(size >= MP4_MIN_HEADER_BYTES as u64, "mp4 output too small");
    let mut header = [0u8; 16];
    let read = file.read(&mut header).await?;
    anyhow::ensure!(read >= MP4_MIN_HEADER_BYTES, "incomplete mp4 header");
    anyhow::ensure!(&header[4..8] == b"ftyp", "missing ISO BMFF ftyp box");
    Ok(())
}

/// Valide l'équivalence stricte de remuxage entre deux arbres de sonde.
pub fn validate_remux_values(before: &Value, after: &Value) -> anyhow::Result<()> {
    anyhow::ensure!(
        track_signature(before) == track_signature(after),
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
        attachment_signature(before) == attachment_signature(after),
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

/// Valide qu'une édition en place n'a modifié ni le nombre ni les codecs des pistes.
pub fn validate_tracks_unchanged_values(before: &Value, after: &Value) -> anyhow::Result<()> {
    anyhow::ensure!(
        track_codecs(before) == track_codecs(after),
        "track layout mismatch after metadata edit"
    );
    Ok(())
}

/// Vérifie qu'un fichier média produit est non vide.
pub async fn ensure_non_empty_file(output: &Path) -> anyhow::Result<()> {
    let meta = tokio::fs::metadata(output).await?;
    anyhow::ensure!(meta.len() > 0, "empty media output");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_run::test_support::fake_tool;
    use serde_json::json;

    #[test]
    fn extracts_duration_from_mkvmerge_and_mediainfo_trees() {
        let mkv = json!({
            "container": { "properties": { "duration": 90_000_000_000u64 } }
        });
        assert_eq!(media_duration_secs(&mkv), Some(90.0));

        let enriched = merge_probe_enrichment(
            mkv,
            "mediainfo",
            json!({ "media": { "track": [{ "@type": "General", "Duration": "45.5" }] } }),
        );
        assert_eq!(media_duration_secs(&enriched), Some(90.0));
        assert_eq!(
            media_duration_secs(&json!({
                "mediainfo": { "media": { "track": [{ "Duration": 12.25 }] } }
            })),
            Some(12.25)
        );
    }

    #[tokio::test]
    async fn validate_mp4_header_requires_ftyp_box() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let valid = dir.path().join("ok.mp4");
        let invalid = dir.path().join("bad.mp4");
        tokio::fs::write(&valid, b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00").await?;
        tokio::fs::write(&invalid, b"not-an-mp4-container").await?;

        validate_mp4_header(&valid).await?;
        assert!(validate_mp4_header(&invalid).await.is_err());
        Ok(())
    }

    #[test]
    fn validate_remux_and_tracks_compare_probe_trees() -> anyhow::Result<()> {
        let src = json!({
            "container": { "recognized": true, "supported": true, "properties": { "title": "Film" } },
            "tracks": [{ "type": "video", "codec": "HEVC", "properties": { "language": "und" } }],
            "attachments": [],
            "chapters": [],
            "global_tags": [],
            "track_tags": []
        });
        let changed_lang = json!({
            "container": { "recognized": true, "supported": true, "properties": { "title": "Film" } },
            "tracks": [{ "type": "video", "codec": "HEVC", "properties": { "language": "fra" } }],
            "attachments": [],
            "chapters": [],
            "global_tags": [],
            "track_tags": []
        });
        validate_remux_values(&src, &src)?;
        assert!(validate_remux_values(&src, &changed_lang).is_err());
        validate_tracks_unchanged_values(&src, &changed_lang)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_probe_json_executes_declarative_probe() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let bin = fake_tool(
            dir.path(),
            "fakeprobe",
            r#"printf '{"container":{"recognized":true,"supported":true},"tracks":[{"type":"video","codec":"HEVC"}]}'"#,
        )?;
        let args = vec!["-J".to_owned(), "video.mkv".to_owned()];
        let val = run_probe_json(ProbeCommand::new("fakeprobe", &bin, &args)).await?;
        assert_eq!(val["tracks"][0]["codec"], "HEVC");
        Ok(())
    }
}
