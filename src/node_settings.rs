//! Rôle du nœud conservé dans `/config` : un worker enrôlé y garde son nom, l'adresse de la centrale
//! et la clé d'API de son nœud (ADR 0011). Sans fichier, le nœud est une centrale (ou n'est pas installé).

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

const SETTINGS_FILE: &str = "node.json";
const WORKER_KEY_FILE: &str = "worker.key";
const TEMPORARY_SUFFIX: &str = ".tmp";
const NODE_NAME_MAX_LEN: usize = 80;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

/// Identité d'un nœud worker enrôlé. La clé d'API vit dans son propre fichier, jamais dans ce JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerNodeSettings {
    pub name: String,
    pub coordinator_url: String,
}

/// Nom de nœud admis : lettres, chiffres, `-`, `_` et `.` (il sert aussi d'identifiant de worker).
pub fn is_valid_node_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NODE_NAME_MAX_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// Écrit `contents` dans `path` de façon atomique (fichier temporaire puis renommage).
pub(crate) fn write_private(path: &Path, contents: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(TEMPORARY_SUFFIX);
    let temporary = std::path::PathBuf::from(temporary);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, PRIVATE_FILE_MODE);
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("écriture de {}", temporary.display()))?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

/// Réglages du worker enrôlé, ou `None` si ce nœud n'est pas un worker enrôlé.
pub fn load_worker(config_dir: &Path) -> anyhow::Result<Option<WorkerNodeSettings>> {
    let path = config_dir.join(SETTINGS_FILE);
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            Ok(Some(serde_json::from_str(&raw).with_context(|| {
                format!("lecture de {}", path.display())
            })?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Clé d'API du nœud, lue dans `/config`.
pub fn load_worker_key(config_dir: &Path) -> anyhow::Result<String> {
    let path = config_dir.join(WORKER_KEY_FILE);
    let key = std::fs::read_to_string(&path)
        .with_context(|| format!("lecture de {}", path.display()))?
        .trim()
        .to_owned();
    anyhow::ensure!(!key.is_empty(), "clé de nœud vide");
    Ok(key)
}

/// Enregistre le rôle worker. La clé est écrite d'abord : la présence de `node.json` garantit
/// qu'elle existe.
pub fn save_worker(
    config_dir: &Path,
    settings: &WorkerNodeSettings,
    key: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(is_valid_node_name(&settings.name), "nom de nœud invalide");
    std::fs::create_dir_all(config_dir)?;
    write_private(&config_dir.join(WORKER_KEY_FILE), key)?;
    write_private(
        &config_dir.join(SETTINGS_FILE),
        &serde_json::to_string_pretty(settings)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> WorkerNodeSettings {
        WorkerNodeSettings {
            name: "nas-1".into(),
            coordinator_url: "http://central:3000".into(),
        }
    }

    #[test]
    fn test_node_name_rules() {
        assert!(is_valid_node_name("nas-1.local_2"));
        for bad in ["", "a b", "é", "a/b", &"x".repeat(NODE_NAME_MAX_LEN + 1)] {
            assert!(!is_valid_node_name(bad), "{bad}");
        }
    }

    #[test]
    fn test_no_file_means_no_worker_role() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        assert_eq!(load_worker(dir.path())?, None);
        Ok(())
    }

    #[test]
    fn test_saved_worker_round_trips_with_its_key() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        save_worker(dir.path(), &settings(), "fdf_secret\n")?;
        assert_eq!(load_worker(dir.path())?, Some(settings()));
        assert_eq!(load_worker_key(dir.path())?, "fdf_secret");
        let json = std::fs::read_to_string(dir.path().join(SETTINGS_FILE))?;
        assert!(!json.contains("fdf_secret"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_key_file_is_private() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        save_worker(dir.path(), &settings(), "fdf_secret")?;
        let mode = std::fs::metadata(dir.path().join(WORKER_KEY_FILE))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, PRIVATE_FILE_MODE);
        Ok(())
    }

    #[test]
    fn test_invalid_name_is_refused_and_nothing_is_written() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let bad = WorkerNodeSettings {
            name: "mauvais nom".into(),
            ..settings()
        };
        assert!(save_worker(dir.path(), &bad, "fdf_secret").is_err());
        assert_eq!(load_worker(dir.path())?, None);
        assert!(load_worker_key(dir.path()).is_err());
        Ok(())
    }

    #[test]
    fn test_corrupted_settings_are_reported() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join(SETTINGS_FILE), "pas du json")?;
        assert!(load_worker(dir.path()).is_err());
        Ok(())
    }
}
