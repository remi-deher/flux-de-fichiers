use crate::{
    domain::MediaFile,
    error::{AppError, AppResult},
};
use std::path::{Component, Path, PathBuf};

pub fn valid_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(Component::Normal(_)))
        && parts.next().is_none()
        && !name.starts_with('.')
        && !name.contains(['\\', '\0'])
        && name.len() <= 240
        && name.to_ascii_lowercase().ends_with(".mkv")
}

pub async fn input_path(root: &Path, name: &str) -> AppResult<PathBuf> {
    if !valid_name(name) {
        return Err(AppError::Invalid);
    }
    let path = root.join(name);
    let metadata = tokio::fs::symlink_metadata(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound
        } else {
            AppError::Storage(e)
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::Invalid);
    }
    Ok(path)
}

pub async fn list_inputs(root: &Path, max_bytes: u64) -> AppResult<Vec<MediaFile>> {
    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_name(&name) || !entry.file_type().await?.is_file() {
            continue;
        }
        let bytes = entry.metadata().await?.len();
        if bytes > 0 && bytes <= max_bytes {
            files.push(MediaFile { name, bytes });
        }
        if files.len() >= 500 {
            break;
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

pub async fn fingerprint(path: &Path, limit: u64) -> AppResult<(String, u64)> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > limit {
            return Err(AppError::TooLarge);
        }
        hash.update(&buffer[..n]);
    }
    if total == 0 {
        return Err(AppError::Invalid);
    }
    Ok((format!("{:x}", hash.finalize()), total))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_flat_mkv_names_are_accepted() {
        for name in [
            "../movie.mkv",
            "/movie.mkv",
            "a/b.mkv",
            "a\\b.mkv",
            ".hidden.mkv",
            "x.mp4",
            "",
        ] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(valid_name("Exemple vidéo.MKV"));
    }
    #[tokio::test]
    async fn rejects_symlink_inputs() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(dir.path().join("source.mkv"), b"test").await?;
        std::os::unix::fs::symlink("source.mkv", dir.path().join("link.mkv"))?;
        assert!(input_path(dir.path(), "link.mkv").await.is_err());
        assert_eq!(list_inputs(dir.path(), 100).await?.len(), 1);
        Ok(())
    }
}
