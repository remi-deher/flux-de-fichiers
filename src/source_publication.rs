//! Publication d'un résultat auprès du détenteur de la source.

use crate::{
    domain::LibraryOutputPolicy,
    error::{AppError, AppResult},
    storage,
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Copie le résultat à côté de la source et refuse de remplacer une source modifiée.
/// La copie temporaire est sur le même système de fichiers que la destination.
#[allow(clippy::too_many_arguments)]
pub async fn apply_library_output_policy(
    policy: LibraryOutputPolicy,
    source: &Path,
    output: &Path,
    extension: &str,
    expected_hash: &str,
    expected_bytes: i64,
    max_bytes: u64,
    authorize: impl std::future::Future<Output = AppResult<()>>,
) -> AppResult<()> {
    if policy == LibraryOutputPolicy::OutputDir {
        return Ok(());
    }
    let target = source.with_extension(extension);
    let temp = source.with_file_name(format!(".forgearr-replace-{}.tmp", Uuid::new_v4()));
    let result = async {
        let metadata = tokio::fs::metadata(source).await?;
        let mut input = tokio::fs::File::open(output).await?;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .await?;
        tokio::io::copy(&mut input, &mut file).await?;
        file.set_permissions(metadata.permissions()).await?;
        file.sync_all().await?;
        drop(file);
        let (hash, bytes) = storage::fingerprint(source, max_bytes).await?;
        if hash != expected_hash || i64::try_from(bytes).ok() != Some(expected_bytes) {
            return Err(AppError::Conflict);
        }
        authorize.await?;
        if policy == LibraryOutputPolicy::ReplaceWithBackup {
            let mut backup = source.as_os_str().to_owned();
            backup.push(".bak");
            tokio::fs::hard_link(source, PathBuf::from(backup)).await?;
        }
        if target == source {
            tokio::fs::rename(&temp, &target).await?;
        } else {
            tokio::fs::hard_link(&temp, &target).await?;
            tokio::fs::remove_file(source).await?;
        }
        Ok(())
    }
    .await;
    let _ = tokio::fs::remove_file(temp).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_BYTES: u64 = 1024;

    #[tokio::test]
    async fn refuses_changed_source_and_preserves_it() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("film.mkv");
        let output = directory.path().join("result.mkv");
        std::fs::write(&source, b"original")?;
        std::fs::write(&output, b"result")?;
        let (hash, bytes) = storage::fingerprint(&source, MAX_BYTES).await?;
        std::fs::write(&source, b"modified")?;
        assert!(matches!(
            apply_library_output_policy(
                LibraryOutputPolicy::ReplaceWithBackup,
                &source,
                &output,
                "mkv",
                &hash,
                bytes as i64,
                MAX_BYTES,
                async { Ok(()) },
            )
            .await,
            Err(AppError::Conflict)
        ));
        assert_eq!(std::fs::read(&source)?, b"modified");
        assert!(!directory.path().join("film.mkv.bak").exists());
        Ok(())
    }

    #[tokio::test]
    async fn backup_and_collision_keep_original_contents() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("film.mp4");
        let target = directory.path().join("film.mkv");
        let output = directory.path().join("result.mkv");
        std::fs::write(&source, b"source")?;
        std::fs::write(&target, b"other")?;
        std::fs::write(&output, b"result")?;
        let (hash, bytes) = storage::fingerprint(&source, MAX_BYTES).await?;
        assert!(
            apply_library_output_policy(
                LibraryOutputPolicy::ReplaceInPlace,
                &source,
                &output,
                "mkv",
                &hash,
                bytes as i64,
                MAX_BYTES,
                async { Ok(()) },
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&source)?, b"source");
        assert_eq!(std::fs::read(&target)?, b"other");
        apply_library_output_policy(
            LibraryOutputPolicy::ReplaceWithBackup,
            &source,
            &output,
            "mp4",
            &hash,
            bytes as i64,
            MAX_BYTES,
            async { Ok(()) },
        )
        .await?;
        assert_eq!(std::fs::read(&source)?, b"result");
        assert_eq!(
            std::fs::read(directory.path().join("film.mp4.bak"))?,
            b"source"
        );
        Ok(())
    }

    #[tokio::test]
    async fn denied_commit_leaves_the_source_and_backup_untouched() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("film.mkv");
        let output = directory.path().join("result.mkv");
        std::fs::write(&source, b"source")?;
        std::fs::write(&output, b"result")?;
        let (hash, bytes) = storage::fingerprint(&source, MAX_BYTES).await?;
        assert!(matches!(
            apply_library_output_policy(
                LibraryOutputPolicy::ReplaceWithBackup,
                &source,
                &output,
                "mkv",
                &hash,
                bytes as i64,
                MAX_BYTES,
                async { Err(AppError::Conflict) },
            )
            .await,
            Err(AppError::Conflict)
        ));
        assert_eq!(std::fs::read(&source)?, b"source");
        assert!(!directory.path().join("film.mkv.bak").exists());
        Ok(())
    }
}
