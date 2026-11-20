use crate::{
    domain::MediaFile,
    error::{AppError, AppResult},
};
use std::path::{Component, Path, PathBuf};

const MAX_OUTPUT_EXTENSION_LENGTH: usize = 16;

/// Une extension ne doit jamais contenir de séparateur ou de composant de chemin.
pub fn valid_output_extension(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= MAX_OUTPUT_EXTENSION_LENGTH
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub fn valid_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(Component::Normal(_)))
        && parts.next().is_none()
        && !name.starts_with('.')
        && !name.contains(['\\', '\0'])
        && name.len() <= 240
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

/// Un fichier de travail transmis doit rester dans le dossier privé du bail.
pub async fn verified_working_path(path: &Path, work_dir: &Path) -> AppResult<PathBuf> {
    let root = tokio::fs::canonicalize(work_dir).await?;
    let file = tokio::fs::canonicalize(path).await?;
    if !file.starts_with(&root) || !tokio::fs::metadata(&file).await?.is_file() {
        return Err(AppError::Invalid);
    }
    Ok(file)
}

/// Rend durable la création ou le renommage d'un fichier dans un répertoire.
pub async fn sync_directory(path: &Path) -> AppResult<()> {
    tokio::fs::File::open(path).await?.sync_all().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_extension_is_a_bounded_alphanumeric_suffix() {
        assert!(valid_output_extension("mkv"));
        assert!(valid_output_extension("MP4"));
        for invalid in [
            "",
            ".mkv",
            "../mkv",
            "a/b",
            "a\\b",
            "a b",
            "abcdefghijklmnopq",
        ] {
            assert!(!valid_output_extension(invalid));
        }
    }
    #[test]
    fn only_flat_non_hidden_names_are_accepted() {
        for name in [
            "../movie.mkv",
            "/movie.mkv",
            "a/b.mkv",
            "a\\b.mkv",
            ".hidden.mkv",
            "",
        ] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(valid_name("Exemple vidéo.MKV"));
        assert!(valid_name("x.mp4"));
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

    #[tokio::test]
    async fn working_path_rejects_a_symlink_outside_the_job_directory() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let work = dir.path().join("work");
        tokio::fs::create_dir(&work).await?;
        let inside = work.join("working.mkv");
        let outside = dir.path().join("outside.mkv");
        tokio::fs::write(&inside, b"inside").await?;
        tokio::fs::write(&outside, b"outside").await?;
        std::os::unix::fs::symlink(&outside, work.join("link.mkv"))?;
        assert_eq!(
            verified_working_path(&inside, &work).await?,
            tokio::fs::canonicalize(&inside).await?
        );
        assert!(
            verified_working_path(&work.join("link.mkv"), &work)
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn syncing_an_existing_directory_succeeds() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(dir.path().join("file"), b"data").await?;
        sync_directory(dir.path()).await?;
        Ok(())
    }
}
