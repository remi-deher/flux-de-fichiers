//! Sonde d'encodeurs logiciels à partir d'un contrat d'application installé par un plugin.

use crate::{
    error::{AppError, AppResult},
    tools_cache::{ToolCacheManager, ToolDistribution, host_arch},
};
use forgearr_shared::capabilities::VideoCodec;
use serde::{Deserialize, Serialize};
use std::{
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{OnceLock, RwLock},
    time::Duration,
};
use tokio::io::AsyncReadExt;

const SOFTWARE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PROBE_OUTPUT_BYTES: u64 = 1024 * 1024;
const CAPABILITY_PROBE_ROLE: &str = "capability-probe";

static SOFTWARE_CAPABILITIES: OnceLock<RwLock<Vec<VideoCodec>>> = OnceLock::new();

/// Correspondance entre un nom d'encodeur déclaré par l'application et un codec Forgearr.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftwareEncoderMapping {
    pub encoder: String,
    pub codec: VideoCodec,
}

/// Sonde fournie par le catalogue : l'exécutable doit être présent et vérifié dans le cache.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareEncoderProbe {
    pub tool_id: String,
    pub tool_version: String,
    pub arch: String,
    pub executable: String,
    pub sha256: String,
    pub args: Vec<String>,
    pub encoders: Vec<SoftwareEncoderMapping>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeDeclaration {
    tool_id: String,
    args: Vec<String>,
    encoders: Vec<SoftwareEncoderMapping>,
}

/// Résout les sondes déclarées dans les plugins en artefacts par architecture.
pub fn discover_software_probes(roots: &[PathBuf]) -> Vec<SoftwareEncoderProbe> {
    let mut probes = Vec::new();
    for (root, entry) in crate::plugins::catalog_with_roots(roots) {
        if entry.entry_type != crate::plugins::CatalogItemType::Plugin
            || !ToolCacheManager::is_safe_relative_path(Path::new(&entry.manifest_path))
        {
            continue;
        }
        let Ok(manifest) = crate::plugins::load_manifest(&root, &entry.manifest_path) else {
            continue;
        };
        let Some(parent) = Path::new(&entry.manifest_path).parent() else {
            continue;
        };
        for file in manifest
            .files
            .iter()
            .filter(|file| file.role.as_deref() == Some(CAPABILITY_PROBE_ROLE))
        {
            if !ToolCacheManager::is_safe_relative_path(Path::new(&file.path)) {
                continue;
            }
            let path = root.join(parent).join(&file.path);
            let (Ok(canonical_root), Ok(canonical_path)) =
                (std::fs::canonicalize(&root), std::fs::canonicalize(&path))
            else {
                continue;
            };
            if !canonical_path.starts_with(canonical_root) {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(canonical_path) else {
                continue;
            };
            let Ok(declaration) = serde_json::from_str::<ProbeDeclaration>(&raw) else {
                continue;
            };
            let Ok(raw_distribution) =
                crate::plugins::read_distribution(roots, &declaration.tool_id)
            else {
                continue;
            };
            let Ok(distribution) = serde_json::from_value::<ToolDistribution>(raw_distribution)
            else {
                continue;
            };
            if distribution.tool_id != declaration.tool_id
                || distribution.release_status.as_deref() == Some("contract-only")
            {
                continue;
            }
            for artifact in distribution.artifacts {
                let Some(sha256) = artifact.binary_sha256 else {
                    continue;
                };
                let probe = SoftwareEncoderProbe {
                    tool_id: distribution.tool_id.clone(),
                    tool_version: distribution.tool_version.clone(),
                    arch: artifact.arch,
                    executable: artifact.binary_relative_path,
                    sha256,
                    args: declaration.args.clone(),
                    encoders: declaration.encoders.clone(),
                };
                if !probes.contains(&probe) {
                    probes.push(probe);
                }
            }
        }
    }
    probes
}

/// Remplace les capacités relevées ; une sonde absente ne laisse pas de capacité périmée.
pub fn set_software_capabilities(mut codecs: Vec<VideoCodec>) {
    codecs.sort();
    codecs.dedup();
    if let Ok(mut current) = SOFTWARE_CAPABILITIES.get_or_init(RwLock::default).write() {
        *current = codecs;
    }
}

pub fn software_capabilities() -> Vec<VideoCodec> {
    SOFTWARE_CAPABILITIES
        .get_or_init(RwLock::default)
        .read()
        .map(|value| value.clone())
        .unwrap_or_default()
}

/// Agrège les résultats des sondes dont l'application est installée sur cet hôte.
pub async fn refresh_software_capabilities(
    cache: &ToolCacheManager,
    probes: &[SoftwareEncoderProbe],
) {
    let mut codecs = Vec::new();
    for probe in probes.iter().filter(|probe| probe.arch == host_arch()) {
        if let Ok(found) = detect_software_encoders(cache, probe).await {
            codecs.extend(found);
        }
    }
    set_software_capabilities(codecs);
}

/// Lit les noms d'encodeurs vidéo de la sortie tabulaire et applique les correspondances du contrat.
pub fn parse_software_encoders(
    output: &str,
    mappings: &[SoftwareEncoderMapping],
) -> Vec<VideoCodec> {
    let mut codecs = Vec::new();
    for line in output.lines() {
        let mut columns = line.split_whitespace();
        let (Some(flags), Some(name)) = (columns.next(), columns.next()) else {
            continue;
        };
        if flags.len() != 6 || !flags.starts_with('V') {
            continue;
        }
        codecs.extend(
            mappings
                .iter()
                .filter(|entry| entry.encoder == name)
                .map(|entry| entry.codec),
        );
    }
    codecs.sort();
    codecs.dedup();
    codecs
}

/// Exécute une sonde bornée après vérification de l'exécutable du cache, jamais depuis le PATH.
pub async fn detect_software_encoders(
    cache: &ToolCacheManager,
    probe: &SoftwareEncoderProbe,
) -> AppResult<Vec<VideoCodec>> {
    for item in [&probe.tool_id, &probe.tool_version] {
        let path = Path::new(item);
        if !matches!(path.components().next(), Some(Component::Normal(_)))
            || path.components().count() != 1
        {
            return Err(AppError::Invalid);
        }
    }
    if probe.arch != host_arch() {
        return Err(AppError::Invalid);
    }
    if !ToolCacheManager::is_safe_relative_path(Path::new(&probe.executable)) {
        return Err(AppError::Invalid);
    }
    let executable = cache.verify_cached_binary(
        &probe.tool_id,
        &probe.tool_version,
        host_arch(),
        &probe.executable,
        Some(&probe.sha256),
    )?;
    let root =
        std::fs::canonicalize(cache.tool_dir(&probe.tool_id, &probe.tool_version, host_arch()))?;
    if !std::fs::canonicalize(&executable)?.starts_with(root) {
        return Err(AppError::Invalid);
    }
    let mut child = tokio::process::Command::new(executable)
        .args(&probe.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child.stdout.take().ok_or(AppError::Internal)?;
    let (status, output) = tokio::time::timeout(SOFTWARE_PROBE_TIMEOUT, async {
        let mut output = Vec::new();
        stdout
            .take(MAX_PROBE_OUTPUT_BYTES + 1)
            .read_to_end(&mut output)
            .await?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, output))
    })
    .await
    .map_err(|_| AppError::CentralUnavailable)??;
    if !status.success() || output.len() as u64 > MAX_PROBE_OUTPUT_BYTES {
        return Err(AppError::Invalid);
    }
    let output = std::str::from_utf8(&output).map_err(|_| AppError::Invalid)?;
    Ok(parse_software_encoders(output, &probe.encoders))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mappings() -> Vec<SoftwareEncoderMapping> {
        vec![
            SoftwareEncoderMapping {
                encoder: "libx265".into(),
                codec: VideoCodec::Hevc,
            },
            SoftwareEncoderMapping {
                encoder: "libsvtav1".into(),
                codec: VideoCodec::Av1,
            },
        ]
    }

    #[test]
    fn parses_only_declared_video_encoders_and_deduplicates() {
        let output = "Encoders:\n V..... libx265 encoder\n V..... libsvtav1 encoder\n V..... libx265 duplicate\n A..... libsvtav1 audio\n V..... hevc_nvenc hardware\n";
        assert_eq!(
            parse_software_encoders(output, &mappings()),
            [VideoCodec::Hevc, VideoCodec::Av1]
        );
        assert!(parse_software_encoders(" V..... hevc_nvenc", &mappings()).is_empty());
    }

    #[tokio::test]
    async fn executes_only_a_verified_cached_binary() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path().to_path_buf());
        let root = cache.tool_dir("fixture", "1.0.0", host_arch());
        std::fs::create_dir_all(root.join("bin"))?;
        let binary = root.join("bin/probe");
        std::fs::write(&binary, b"#!/bin/sh\nprintf ' V..... libx265 encoder\\n'\n")?;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
        std::fs::write(root.join(".ready"), b"")?;
        let mut probe = SoftwareEncoderProbe {
            tool_id: "fixture".into(),
            tool_version: "1.0.0".into(),
            arch: host_arch().into(),
            executable: "probe".into(),
            sha256: ToolCacheManager::compute_file_sha256(&binary)?,
            args: Vec::new(),
            encoders: mappings(),
        };
        assert_eq!(
            detect_software_encoders(&cache, &probe).await?,
            [VideoCodec::Hevc]
        );
        probe.sha256 = "0".repeat(64);
        assert!(detect_software_encoders(&cache, &probe).await.is_err());
        probe.executable = "../outside".into();
        assert!(matches!(
            detect_software_encoders(&cache, &probe).await,
            Err(AppError::Invalid)
        ));
        Ok(())
    }
}
