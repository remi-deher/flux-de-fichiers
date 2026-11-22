//! Chemins et lectures bornés à un dossier de catalogue : un dépôt peut être tiers.

use crate::{
    plugins::{sources::INDEX_RELATIVE_PATH, sync::MAX_FILE_BYTES},
    tools_cache::ToolCacheManager,
};
use std::{
    fs,
    io::{ErrorKind, Read},
    path::{Component, Path},
};

const MAX_RELATIVE_PATH_LEN: usize = 512;

/// Chemin relatif à la racine, sous sa forme simple (`a/b/c`) ; `None` s'il sort du dossier, est
/// illisible ou désigne l'index, que chaque copie figée réécrit.
pub(super) fn safe_path(raw: &str) -> Option<String> {
    let path = Path::new(raw);
    if raw.len() > MAX_RELATIVE_PATH_LEN
        || raw.contains('\\')
        || raw.chars().any(char::is_control)
        || !ToolCacheManager::is_safe_relative_path(path)
    {
        return None;
    }
    let parts: Vec<&str> = path
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let plain = parts.join("/");
    (!plain.is_empty() && plain != INDEX_RELATIVE_PATH).then_some(plain)
}

/// Chemin déclaré par un manifeste, relatif au dossier de ce manifeste.
pub(super) fn declared_path(folder: &str, declared: &str) -> Option<String> {
    if !ToolCacheManager::is_safe_relative_path(Path::new(declared)) {
        return None;
    }
    if folder.is_empty() {
        safe_path(declared)
    } else {
        safe_path(&format!("{folder}/{declared}"))
    }
}

/// Dossier d'un chemin simple ; vide à la racine.
pub(super) fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Lit un fichier de `root` (chemin canonique). `None` s'il est absent, n'est pas un fichier ordinaire,
/// passe par un lien symbolique ou dépasse la taille permise.
pub(super) fn read_inside(root: &Path, relative: &str) -> std::io::Result<Option<Vec<u8>>> {
    let path = root.join(relative);
    let resolved = match fs::canonicalize(&path) {
        Ok(resolved) => resolved,
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if resolved != path || !fs::symlink_metadata(&path)?.is_file() {
        tracing::warn!(path = %path.display(), "lien symbolique ou fichier spécial refusé");
        return Ok(None);
    }
    let mut content = Vec::new();
    fs::File::open(&path)?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut content)?;
    if content.len() > MAX_FILE_BYTES {
        tracing::warn!(path = %path.display(), "fichier trop volumineux");
        return Ok(None);
    }
    Ok(Some(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_paths_inside_the_folder_are_safe() {
        assert_eq!(safe_path("./a//b/c.json").as_deref(), Some("a/b/c.json"));
        assert_eq!(safe_path("a/index.json").as_deref(), Some("a/index.json"));
        for unsafe_path in [
            "",
            ".",
            "../a",
            "a/../../b",
            "/etc/passwd",
            "a\\..\\b",
            "a\nb",
            "index.json",
            "./index.json",
        ] {
            assert_eq!(safe_path(unsafe_path), None, "{unsafe_path:?}");
        }
        assert_eq!(
            declared_path("plugins/x", "ops/a.json").as_deref(),
            Some("plugins/x/ops/a.json")
        );
        assert_eq!(declared_path("", "a.json").as_deref(), Some("a.json"));
        assert_eq!(declared_path("plugins/x", "/a.json"), None);
        assert_eq!(declared_path("plugins/x", "../y/a.json"), None);
        assert_eq!(folder_of("plugins/x/manifest.json"), "plugins/x");
        assert_eq!(folder_of("manifest.json"), "");
    }

    #[test]
    fn links_folders_and_large_files_are_not_read() -> anyhow::Result<()> {
        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("secret"), "x")?;
        let dir = tempfile::tempdir()?;
        let root = fs::canonicalize(dir.path())?;
        fs::create_dir(root.join("sub"))?;
        fs::write(root.join("sub/ok"), "contenu")?;
        fs::write(root.join("gros"), vec![0; MAX_FILE_BYTES + 1])?;
        std::os::unix::fs::symlink(outside.path().join("secret"), root.join("lien"))?;
        std::os::unix::fs::symlink(outside.path(), root.join("dossier"))?;
        assert_eq!(
            read_inside(&root, "sub/ok")?.as_deref(),
            Some(b"contenu".as_slice())
        );
        for refused in [
            "sub",
            "gros",
            "lien",
            "dossier/secret",
            "absent",
            "sub/ok/x",
        ] {
            assert_eq!(read_inside(&root, refused)?, None, "{refused}");
        }
        Ok(())
    }
}
