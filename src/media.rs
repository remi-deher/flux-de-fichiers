use crate::tool_run::{CommandSpec, ProgressParser, ProgressStream, run_command};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const PROBE_DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_DURATION_SCALE: f64 = 1.0;

const fn default_duration_scale() -> f64 {
    DEFAULT_DURATION_SCALE
}

/// Règle déclarative d'extraction de durée depuis un arbre JSON de sonde.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurationRule {
    /// Chemin pointé dans l'arbre JSON (supporte `[*]` et `[idx]`).
    pub path: String,
    /// Facteur multiplicatif pour convertir la valeur brute en secondes (défaut : `1.0`).
    #[serde(default = "default_duration_scale")]
    pub scale: f64,
}

/// Règle déclarative de vérification d'en-tête binaire (magic bytes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MagicBytesRule {
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub ascii: Option<String>,
    #[serde(default)]
    pub hex: Option<String>,
}

impl MagicBytesRule {
    /// Retourne la séquence d'octets attendue à `self.offset`.
    pub fn expected_bytes(&self) -> anyhow::Result<Vec<u8>> {
        if let Some(ascii) = &self.ascii {
            anyhow::ensure!(!ascii.is_empty(), "magic ascii ne peut pas être vide");
            return Ok(ascii.as_bytes().to_vec());
        }
        if let Some(hex) = &self.hex {
            let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
            anyhow::ensure!(
                !clean.is_empty() && clean.len().is_multiple_of(2),
                "magic hex invalide"
            );
            let mut bytes = Vec::with_capacity(clean.len() / 2);
            for pair in clean.as_bytes().as_chunks::<2>().0 {
                let s = std::str::from_utf8(pair)?;
                bytes.push(u8::from_str_radix(s, 16)?);
            }
            return Ok(bytes);
        }
        anyhow::bail!("règle magic-bytes sans champ ascii ni hex")
    }
}

/// Mode de validation post-exécution déclaré par un contrat d'opération (`runtime.postValidate`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PostValidateMode {
    #[default]
    None,
    NonEmpty,
    ValidProbe,
    MagicBytes,
    ProbePreserve,
}

impl PostValidateMode {
    /// Le contrôle passe-t-il par une sonde du catalogue ?
    pub const fn uses_probes(self) -> bool {
        matches!(self, Self::ValidProbe | Self::ProbePreserve)
    }
}

/// Spécification complète de validation post-exécution d'un fichier produit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostValidateSpec {
    #[serde(default)]
    pub mode: PostValidateMode,
    #[serde(default)]
    pub magic: Option<MagicBytesRule>,
    #[serde(default)]
    pub preserve_paths: Vec<String>,
}

/// Contrat de sortie effectif d'une opération ou d'un flux (extension, MIME et validation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpec {
    pub extension: String,
    pub content_type: String,
    #[serde(default)]
    pub post_validate: PostValidateSpec,
}

impl Default for OutputSpec {
    fn default() -> Self {
        Self {
            extension: "bin".to_owned(),
            content_type: "application/octet-stream".to_owned(),
            post_validate: PostValidateSpec::default(),
        }
    }
}

/// Format attendu d'une sonde déclarée (`runtime.probeFormat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeFormat {
    #[default]
    Json,
}

/// Paramètres nécessaires pour exécuter une sonde binaire déclarative sur un chemin média.
#[derive(Clone, Debug)]
pub struct ProbeCommand<'a> {
    pub label: &'a str,
    pub bin: &'a Path,
    pub args: &'a [String],
    pub format: ProbeFormat,
    pub require_paths: &'a [String],
    pub success_exit_codes: &'a [i32],
    pub timeout: Duration,
}

impl<'a> ProbeCommand<'a> {
    pub fn new(label: &'a str, bin: &'a Path, args: &'a [String]) -> Self {
        Self {
            label,
            bin,
            args,
            format: ProbeFormat::Json,
            require_paths: &[],
            success_exit_codes: &[0, 1],
            timeout: Duration::from_secs(PROBE_DEFAULT_TIMEOUT_SECS),
        }
    }
}

/// Exécute une sonde binaire et valide la structure JSON selon son contrat.
pub async fn run_probe_json(cmd: ProbeCommand<'_>) -> anyhow::Result<Value> {
    let mut spec = CommandSpec::new(cmd.label, cmd.bin, cmd.args);
    spec.progress_parser = ProgressParser::None;
    spec.progress_stream = ProgressStream::Stderr;
    spec.success_exit_codes = cmd.success_exit_codes;
    spec.timeout = cmd.timeout;
    spec.capture_stdout = true;

    let outcome = run_command(spec, |_, _| {}).await?;
    let data: Value = serde_json::from_slice(&outcome.stdout)?;
    validate_probe_json(cmd.format, &data, cmd.require_paths)?;
    Ok(data)
}

fn resolve_json_path<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = data;
    for segment in path.split('.').filter(|s| !s.is_empty()) {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Valide la structure JSON minimale retournée par une sonde et vérifie `require_paths`.
pub fn validate_probe_json(
    format: ProbeFormat,
    data: &Value,
    require_paths: &[String],
) -> anyhow::Result<()> {
    match format {
        ProbeFormat::Json => {
            anyhow::ensure!(data.is_object() || data.is_array(), "invalid probe json");
        }
    }
    for path in require_paths {
        let value = resolve_json_path(data, path)
            .ok_or_else(|| anyhow::anyhow!("missing required probe path: {path}"))?;
        anyhow::ensure!(
            !matches!(value, Value::Null | Value::Bool(false)),
            "probe path {path} is null or false"
        );
        if let Some(items) = value.as_array() {
            anyhow::ensure!(!items.is_empty(), "probe path {path} is empty");
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

fn collect_json_path<'a>(current: &'a Value, segments: &[&str], out: &mut Vec<Option<&'a Value>>) {
    let Some((&head, rest)) = segments.split_first() else {
        out.push(Some(current));
        return;
    };
    if let Some(key) = head.strip_suffix("[*]") {
        let target = if key.is_empty() {
            Some(current)
        } else {
            current.get(key)
        };
        if let Some(items) = target.and_then(Value::as_array) {
            for item in items {
                collect_json_path(item, rest, out);
            }
        }
        return;
    }
    if let Some(bracket) = head.find('[')
        && let Some(idx_str) = head[bracket + 1..].strip_suffix(']')
        && let Ok(idx) = idx_str.parse::<usize>()
    {
        let key = &head[..bracket];
        let target = if key.is_empty() {
            Some(current)
        } else {
            current.get(key)
        };
        match target
            .and_then(Value::as_array)
            .and_then(|arr| arr.get(idx))
        {
            Some(next) => collect_json_path(next, rest, out),
            None => out.push(None),
        }
        return;
    }
    match current.get(head) {
        Some(next) => collect_json_path(next, rest, out),
        None => out.push(None),
    }
}

fn resolve_path_values<'a>(data: &'a Value, path: &str) -> Vec<Option<&'a Value>> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let mut out = Vec::new();
    collect_json_path(data, &segments, &mut out);
    out
}

/// Extrait la durée totale en secondes depuis un arbre JSON selon les règles déclarées.
pub fn extract_duration_secs(info: &Value, rules: &[DurationRule]) -> Option<f64> {
    for rule in rules {
        if rule.scale <= 0.0 || !rule.scale.is_finite() {
            continue;
        }
        for value in resolve_path_values(info, &rule.path).into_iter().flatten() {
            let raw = value
                .as_f64()
                .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()));
            if let Some(val) = raw
                && val > 0.0
                && val.is_finite()
            {
                return Some(val * rule.scale);
            }
        }
    }
    None
}

/// Valide qu'un fichier produit contient les octets magiques attendus à l'offset configuré.
pub async fn validate_magic_bytes(output: &Path, rule: &MagicBytesRule) -> anyhow::Result<()> {
    let expected = rule.expected_bytes()?;
    let mut file = tokio::fs::File::open(output).await?;
    let size = file.metadata().await?.len();
    let required_end = (rule.offset as u64)
        .checked_add(expected.len() as u64)
        .ok_or_else(|| anyhow::anyhow!("magic bytes offset overflow"))?;
    anyhow::ensure!(size >= required_end, "output too small for magic header");
    if rule.offset > 0 {
        file.seek(std::io::SeekFrom::Start(rule.offset as u64))
            .await?;
    }
    let mut actual = vec![0u8; expected.len()];
    file.read_exact(&mut actual).await?;
    anyhow::ensure!(
        actual == expected,
        "magic bytes mismatch at offset {}",
        rule.offset
    );
    Ok(())
}

/// Vérifie que tous les chemins déclarés dans `preserve_paths` sont identiques entre `before` et `after`.
pub fn validate_probe_preserved(
    before: &Value,
    after: &Value,
    preserve_paths: &[String],
) -> anyhow::Result<()> {
    for path in preserve_paths {
        let left = resolve_path_values(before, path);
        let right = resolve_path_values(after, path);
        anyhow::ensure!(left == right, "preserved probe path mismatch: {path}");
    }
    Ok(())
}

/// Vérifie qu'un fichier produit est non vide.
pub async fn ensure_non_empty_file(output: &Path) -> anyhow::Result<()> {
    let meta = tokio::fs::metadata(output).await?;
    anyhow::ensure!(meta.len() > 0, "empty output file");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_run::test_support::fake_tool;
    use serde_json::json;

    #[test]
    fn extracts_duration_from_declarative_rules() {
        let rules = vec![
            DurationRule {
                path: "container.properties.duration".to_owned(),
                scale: 1e-9,
            },
            DurationRule {
                path: "mediainfo.media.track[*].Duration".to_owned(),
                scale: 1.0,
            },
        ];
        let primary = json!({
            "container": { "properties": { "duration": 90_000_000_000u64 } }
        });
        assert_eq!(extract_duration_secs(&primary, &rules), Some(90.0));

        let enriched = merge_probe_enrichment(
            json!({}),
            "mediainfo",
            json!({ "media": { "track": [{ "@type": "General", "Duration": "45.5" }] } }),
        );
        assert_eq!(extract_duration_secs(&enriched, &rules), Some(45.5));
        assert_eq!(extract_duration_secs(&json!({}), &rules), None);
    }

    #[tokio::test]
    async fn validate_magic_bytes_checks_ascii_and_hex() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let valid = dir.path().join("ok.bin");
        let invalid = dir.path().join("bad.bin");
        tokio::fs::write(&valid, b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00").await?;
        tokio::fs::write(&invalid, b"not-a-valid-header").await?;

        let ascii_rule = MagicBytesRule {
            offset: 4,
            ascii: Some("ftyp".to_owned()),
            hex: None,
        };
        validate_magic_bytes(&valid, &ascii_rule).await?;
        assert!(validate_magic_bytes(&invalid, &ascii_rule).await.is_err());

        let hex_rule = MagicBytesRule {
            offset: 4,
            ascii: None,
            hex: Some("66747970".to_owned()),
        };
        validate_magic_bytes(&valid, &hex_rule).await?;
        Ok(())
    }

    #[test]
    fn validate_probe_preserved_compares_declared_paths() -> anyhow::Result<()> {
        let src = json!({
            "container": { "properties": { "title": "Film" } },
            "tracks": [{ "type": "video", "codec": "HEVC", "properties": { "language": "und" } }],
            "chapters": []
        });
        let changed_lang = json!({
            "container": { "properties": { "title": "Film" } },
            "tracks": [{ "type": "video", "codec": "HEVC", "properties": { "language": "fra" } }],
            "chapters": []
        });
        let strict_paths = vec![
            "container.properties.title".to_owned(),
            "tracks[*].type".to_owned(),
            "tracks[*].codec".to_owned(),
            "tracks[*].properties.language".to_owned(),
            "chapters".to_owned(),
        ];
        let layout_paths = vec!["tracks[*].type".to_owned(), "tracks[*].codec".to_owned()];

        validate_probe_preserved(&src, &src, &strict_paths)?;
        assert!(validate_probe_preserved(&src, &changed_lang, &strict_paths).is_err());
        validate_probe_preserved(&src, &changed_lang, &layout_paths)?;
        Ok(())
    }

    #[test]
    fn validate_probe_json_checks_required_paths() -> anyhow::Result<()> {
        let valid = json!({ "container": { "recognized": true }, "tracks": [{ "id": 0 }] });
        let reqs = vec!["container.recognized".to_owned(), "tracks".to_owned()];
        validate_probe_json(ProbeFormat::Json, &valid, &reqs)?;

        let empty_tracks = json!({ "container": { "recognized": true }, "tracks": [] });
        assert!(validate_probe_json(ProbeFormat::Json, &empty_tracks, &reqs).is_err());

        let false_flag = json!({ "container": { "recognized": false }, "tracks": [{ "id": 0 }] });
        assert!(validate_probe_json(ProbeFormat::Json, &false_flag, &reqs).is_err());

        assert!(validate_probe_json(ProbeFormat::Json, &json!({}), &reqs).is_err());
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
