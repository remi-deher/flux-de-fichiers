use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

const EXECUTABLE_UNIX_MODE: u32 = 0o755;
const DEFAULT_SAFE_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// Déclaration d'un artefact d'outil pour une architecture donnée.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolArtifact {
    pub arch: String,
    #[serde(alias = "archive_url")]
    pub archive_url: String,
    #[serde(alias = "archive_format")]
    pub archive_format: String,
    #[serde(alias = "archive_sha256")]
    pub archive_sha256: String,
    #[serde(alias = "binary_relative_path")]
    pub binary_relative_path: String,
    #[serde(default, alias = "binary_sha256")]
    pub binary_sha256: Option<String>,
    #[serde(default)]
    pub permissions: Option<ArtifactPermissions>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPermissions {
    #[serde(alias = "unix_mode")]
    pub unix_mode: Option<String>,
    pub owner: Option<String>,
    #[serde(alias = "executable_bit")]
    pub executable_bit: Option<bool>,
}

/// Fiche déclarative de distribution d'un outil (Option B).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDistribution {
    #[serde(default, alias = "schema_version")]
    pub schema_version: Option<String>,
    #[serde(alias = "tool_id")]
    pub tool_id: String,
    #[serde(alias = "tool_version")]
    pub tool_version: String,
    #[serde(default = "default_distribution_mode", alias = "distribution_mode")]
    pub distribution_mode: String,
    pub artifacts: Vec<ToolArtifact>,
}

fn default_distribution_mode() -> String {
    "autonomous-binary".to_string()
}

/// Identifie l'architecture cible du système hôte en cours d'exécution.
pub fn host_arch() -> &'static str {
    std::env::consts::ARCH
}

/// Gestionnaire local des outils distribués sur le worker.
#[derive(Clone, Debug)]
pub struct ToolCacheManager {
    cache_root: PathBuf,
}

impl ToolCacheManager {
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
        }
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Calcule le répertoire de stockage canonique d'un outil pour une architecture :
    /// `<cache_root>/<tool_id>/<tool_version>/<arch>/`
    pub fn tool_dir(&self, tool_id: &str, tool_version: &str, arch: &str) -> PathBuf {
        self.cache_root.join(tool_id).join(tool_version).join(arch)
    }

    /// Indique si l'outil est déjà installé, intègre et prêt à l'emploi (`.ready`).
    pub fn is_ready(&self, tool_id: &str, tool_version: &str, arch: &str) -> bool {
        let dir = self.tool_dir(tool_id, tool_version, arch);
        dir.join(".ready").is_file()
    }

    /// Vérifie et retourne le chemin d'un binaire déjà en cache chaud.
    pub fn verify_cached_binary(
        &self,
        tool_id: &str,
        tool_version: &str,
        arch: &str,
        binary_relative_path: &str,
        expected_sha256: Option<&str>,
    ) -> AppResult<PathBuf> {
        let dir = self.tool_dir(tool_id, tool_version, arch);
        if !dir.join(".ready").is_file() {
            return Err(AppError::ToolUnavailable(format!(
                "{tool_id}@{tool_version} ({arch}) not ready"
            )));
        }

        let binary_path = dir.join("bin").join(binary_relative_path);
        if !binary_path.is_file() {
            return Err(AppError::NotFound);
        }

        if let Some(expected) = expected_sha256 {
            let actual = Self::compute_file_sha256(&binary_path)?;
            if !bool::from(subtle::ConstantTimeEq::ct_eq(
                actual.as_bytes(),
                expected.to_lowercase().as_bytes(),
            )) {
                return Err(AppError::ChecksumMismatch {
                    expected: expected.to_string(),
                    actual,
                });
            }
        }

        Ok(binary_path)
    }

    /// Résout le chemin d'un exécutable : vérifie d'abord le cache chaud Option B,
    /// puis se replie sur une variable d'environnement ou le binaire système disponible.
    pub fn resolve_executable(
        &self,
        tool_id: &str,
        tool_version: &str,
        binary_relative_path: &str,
        fallback_env_var: &str,
    ) -> PathBuf {
        let arch = host_arch();
        if let Ok(cached) =
            self.verify_cached_binary(tool_id, tool_version, arch, binary_relative_path, None)
        {
            return cached;
        }
        if let Ok(env_bin) = std::env::var(fallback_env_var) {
            return PathBuf::from(env_bin);
        }
        PathBuf::from(binary_relative_path)
    }

    /// Extrait nativement une archive en mémoire (tar.gz), vérifie les empreintes SHA-256,
    /// attribue les permissions 0755 et crée le marqueur `.ready`.
    /// L'opération utilise un répertoire temporaire et un renommage atomique (zéro verrou .lock).
    pub fn extract_and_install(
        &self,
        tool_id: &str,
        tool_version: &str,
        artifact: &ToolArtifact,
        archive_bytes: &[u8],
    ) -> AppResult<PathBuf> {
        let target_dir = self.tool_dir(tool_id, tool_version, &artifact.arch);
        if target_dir.join(".ready").is_file() {
            let binary_path = target_dir.join("bin").join(&artifact.binary_relative_path);
            if binary_path.is_file() {
                return Ok(binary_path);
            }
        }

        let parent_dir = target_dir.parent().ok_or(AppError::NotFound)?;
        fs::create_dir_all(parent_dir)?;

        let tmp_dir = parent_dir.join(format!(
            ".tmp-{}-{}",
            artifact.arch,
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&tmp_dir)?;

        match self.do_extract_and_install(&tmp_dir, artifact, archive_bytes) {
            Ok(_) => {
                if target_dir.exists() {
                    let _ = fs::remove_dir_all(&target_dir);
                }
                if let Err(e) = fs::rename(&tmp_dir, &target_dir) {
                    let _ = fs::remove_dir_all(&tmp_dir);
                    return Err(AppError::Storage(e));
                }
                let binary_path = target_dir.join("bin").join(&artifact.binary_relative_path);
                Ok(binary_path)
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                Err(e)
            }
        }
    }

    fn do_extract_and_install(
        &self,
        target_dir: &Path,
        artifact: &ToolArtifact,
        archive_bytes: &[u8],
    ) -> AppResult<PathBuf> {
        // 1. Validation de l'empreinte de l'archive
        let actual_archive_sha256 = Self::compute_sha256(archive_bytes);
        if !bool::from(subtle::ConstantTimeEq::ct_eq(
            actual_archive_sha256.as_bytes(),
            artifact.archive_sha256.to_lowercase().as_bytes(),
        )) {
            return Err(AppError::ChecksumMismatch {
                expected: artifact.archive_sha256.clone(),
                actual: actual_archive_sha256,
            });
        }

        if artifact.archive_format != "tar.gz" {
            return Err(AppError::UnsupportedArchive(
                artifact.archive_format.clone(),
            ));
        }

        // 2. Extraction native sans shell via tar et flate2
        let gz_decoder = flate2::read::GzDecoder::new(archive_bytes);
        let mut archive = tar::Archive::new(gz_decoder);

        let bin_dir = target_dir.join("bin");
        fs::create_dir_all(&bin_dir)?;

        for entry_result in archive.entries()? {
            let mut entry = entry_result?;
            let entry_path = entry.path()?.to_path_buf();

            // Seuls les fichiers réguliers et les dossiers sont extraits : un lien symbolique
            // pourrait rediriger une écriture suivante hors du cache.
            let entry_type = entry.header().entry_type();
            if !(entry_type.is_file() || entry_type.is_dir()) {
                return Err(AppError::InvalidManifest(format!(
                    "unsupported entry type in archive: {}",
                    entry_path.display()
                )));
            }

            // Protection stricte contre la traversée de répertoires (Zip Slip)
            if !Self::is_safe_relative_path(&entry_path) {
                return Err(AppError::InvalidManifest(format!(
                    "unsafe path in archive: {}",
                    entry_path.display()
                )));
            }

            let destination = bin_dir.join(&entry_path);
            if entry.header().entry_type().is_dir() {
                fs::create_dir_all(&destination)?;
            } else {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                entry.unpack(&destination)?;
            }
        }

        let binary_path = bin_dir.join(&artifact.binary_relative_path);
        if !binary_path.is_file() {
            return Err(AppError::NotFound);
        }

        // 3. Attribution des permissions d'exécution Unix minimales (0755)
        fs::set_permissions(
            &binary_path,
            fs::Permissions::from_mode(EXECUTABLE_UNIX_MODE),
        )?;

        // 4. Validation de l'empreinte du binaire extrait (si renseignée)
        let actual_binary_sha256 = Self::compute_file_sha256(&binary_path)?;
        if let Some(expected_binary_sha256) = &artifact.binary_sha256 {
            let matches = bool::from(subtle::ConstantTimeEq::ct_eq(
                actual_binary_sha256.as_bytes(),
                expected_binary_sha256.to_lowercase().as_bytes(),
            ));
            if !matches {
                return Err(AppError::ChecksumMismatch {
                    expected: expected_binary_sha256.clone(),
                    actual: actual_binary_sha256,
                });
            }
        }

        // 5. Finalisation : écriture des marqueurs .sha256 et .ready
        fs::write(target_dir.join(".sha256"), &actual_binary_sha256)?;
        fs::write(target_dir.join(".ready"), b"ready")?;

        Ok(binary_path)
    }

    /// Prépare une commande asynchrone sécurisée :
    /// environnement nettoyé (`env_clear`), non-root, sans invocation Docker hôte.
    pub fn prepare_command(
        &self,
        binary_path: &Path,
        args: &[&str],
        work_dir: &Path,
    ) -> AppResult<tokio::process::Command> {
        if !binary_path.is_file() {
            return Err(AppError::NotFound);
        }

        let mut cmd = tokio::process::Command::new(binary_path);
        cmd.env_clear()
            .env("PATH", DEFAULT_SAFE_PATH)
            .env("TMPDIR", work_dir.join("tmp").to_str().unwrap_or("/tmp"))
            .current_dir(work_dir)
            .kill_on_drop(true);

        for arg in args {
            cmd.arg(arg);
        }

        Ok(cmd)
    }

    /// Calcule la somme de contrôle SHA-256 d'un tampon d'octets.
    pub fn compute_sha256(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self::hex_encode(hasher.finalize())
    }

    /// Calcule la somme de contrôle SHA-256 d'un fichier sur disque en flux.
    pub fn compute_file_sha256(path: &Path) -> std::io::Result<String> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(Self::hex_encode(hasher.finalize()))
    }

    fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    }

    /// Vérifie qu'un chemin relatif est sécurisé (pas de `..`, pas de racine `/`).
    pub fn is_safe_relative_path(path: &Path) -> bool {
        for component in path.components() {
            match component {
                Component::Normal(_) => continue,
                Component::CurDir => continue,
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tempfile::tempdir;

    fn build_test_archive(
        binary_name: &str,
        binary_content: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut tar_builder = tar::Builder::new(Vec::new());

        let mut header = tar::Header::new_gnu();
        header.set_size(binary_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        tar_builder.append_data(&mut header, binary_name, binary_content)?;

        let tar_bytes = tar_builder.into_inner()?;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_bytes)?;
        Ok(encoder.finish()?)
    }

    #[test]
    fn test_extract_install_and_verify_cache_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempdir()?;
        let manager = ToolCacheManager::new(tmp.path());

        let bin_content = b"#!/bin/sh\necho 'hello world'\n";
        let archive_bytes = build_test_archive("mkvmerge", bin_content)?;
        let archive_sha256 = ToolCacheManager::compute_sha256(&archive_bytes);
        let binary_sha256 = ToolCacheManager::compute_sha256(bin_content);

        let artifact = ToolArtifact {
            arch: "x86_64".into(),
            archive_url: "https://example.local/mkvmerge.tar.gz".into(),
            archive_format: "tar.gz".into(),
            archive_sha256,
            binary_relative_path: "mkvmerge".into(),
            binary_sha256: Some(binary_sha256.clone()),
            permissions: Some(ArtifactPermissions {
                unix_mode: Some("0755".into()),
                owner: Some("runtime-user".into()),
                executable_bit: Some(true),
            }),
        };

        assert!(!manager.is_ready("app.mkvtoolnix", "88.0.0", "x86_64"));

        let installed_path =
            manager.extract_and_install("app.mkvtoolnix", "88.0.0", &artifact, &archive_bytes)?;

        assert!(installed_path.is_file());
        assert!(manager.is_ready("app.mkvtoolnix", "88.0.0", "x86_64"));

        // Vérification des permissions 0755
        let metadata = fs::metadata(&installed_path)?;
        assert_eq!(metadata.permissions().mode() & 0o777, 0o755);

        // Vérification cache chaud
        let cached_path = manager.verify_cached_binary(
            "app.mkvtoolnix",
            "88.0.0",
            "x86_64",
            "mkvmerge",
            Some(&binary_sha256),
        )?;
        assert_eq!(installed_path, cached_path);
        Ok(())
    }

    #[test]
    fn test_reject_archive_with_checksum_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempdir()?;
        let manager = ToolCacheManager::new(tmp.path());

        let bin_content = b"fake binary";
        let archive_bytes = build_test_archive("test_tool", bin_content)?;

        let artifact = ToolArtifact {
            arch: "x86_64".into(),
            archive_url: "https://example.local/test.tar.gz".into(),
            archive_format: "tar.gz".into(),
            archive_sha256: "0000000000000000000000000000000000000000000000000000000000000000"
                .into(),
            binary_relative_path: "test_tool".into(),
            binary_sha256: None,
            permissions: None,
        };

        let result = manager.extract_and_install("test.tool", "1.0.0", &artifact, &archive_bytes);

        assert!(matches!(result, Err(AppError::ChecksumMismatch { .. })));
        assert!(!manager.is_ready("test.tool", "1.0.0", "x86_64"));
        Ok(())
    }

    #[test]
    fn test_reject_archive_with_symlink_entry() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempdir()?;
        let manager = ToolCacheManager::new(tmp.path());

        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();
        builder.append_link(&mut header, "lien", "/etc")?;
        let tar_bytes = builder.into_inner()?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_bytes)?;
        let archive_bytes = encoder.finish()?;

        let artifact = ToolArtifact {
            arch: "x86_64".into(),
            archive_url: "https://example.local/lien.tar.gz".into(),
            archive_format: "tar.gz".into(),
            archive_sha256: ToolCacheManager::compute_sha256(&archive_bytes),
            binary_relative_path: "lien".into(),
            binary_sha256: None,
            permissions: None,
        };

        let result = manager.extract_and_install("test.tool", "1.0.0", &artifact, &archive_bytes);

        assert!(matches!(result, Err(AppError::InvalidManifest(_))));
        assert!(!manager.is_ready("test.tool", "1.0.0", "x86_64"));
        Ok(())
    }

    #[test]
    fn test_safe_relative_path() {
        assert!(ToolCacheManager::is_safe_relative_path(Path::new(
            "bin/mkvmerge"
        )));
        assert!(ToolCacheManager::is_safe_relative_path(Path::new(
            "mkvmerge"
        )));
        assert!(!ToolCacheManager::is_safe_relative_path(Path::new(
            "../evil"
        )));
        assert!(!ToolCacheManager::is_safe_relative_path(Path::new(
            "/bin/sh"
        )));
        assert!(!ToolCacheManager::is_safe_relative_path(Path::new(
            "foo/../../bar"
        )));
    }
}
