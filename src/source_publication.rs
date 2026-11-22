//! Publication d'un résultat auprès du détenteur de la source.

use crate::{
    domain::LibraryOutputPolicy,
    error::{AppError, AppResult},
    storage,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use uuid::Uuid;

/// Âge maximal de la dernière vérification de la source au moment de la remplacer : au-delà (machine
/// suspendue entre les deux), la source est vérifiée de nouveau (ADR 0027).
const VERIFICATION_MAX_AGE: Duration = Duration::from_secs(5);

/// L'horloge murale compte aussi le temps de suspension ; un recul d'horloge vaut une péremption.
fn verification_stale(verified_at: SystemTime) -> bool {
    verified_at
        .elapsed()
        .map_or(true, |age| age > VERIFICATION_MAX_AGE)
}

pub(crate) fn missing_source_is_changed(error: AppError) -> AppError {
    match error {
        AppError::NotFound => AppError::SourceChanged,
        AppError::Storage(ref io_error) if io_error.kind() == std::io::ErrorKind::NotFound => {
            AppError::SourceChanged
        }
        other => other,
    }
}

pub(crate) async fn verify_source(
    source: &Path,
    expected_hash: &str,
    expected_bytes: i64,
    max_bytes: u64,
) -> AppResult<()> {
    let fingerprint =
        storage::fingerprint(source, max_bytes)
            .await
            .map_err(|error| match error {
                AppError::Invalid | AppError::TooLarge => AppError::SourceChanged,
                other => missing_source_is_changed(other),
            })?;
    if fingerprint.0 != expected_hash || i64::try_from(fingerprint.1).ok() != Some(expected_bytes) {
        return Err(AppError::SourceChanged);
    }
    Ok(())
}

/// Éléments nécessaires pour constater un remplacement sans modifier la source.
pub struct PublishedSourceExpectation<'a> {
    pub root: &'a Path,
    pub source_name: &'a str,
    pub source_sha256: &'a str,
    pub source_bytes: i64,
    pub result_sha256: &'a str,
    pub result_bytes: i64,
    pub extension: &'a str,
    pub policy: LibraryOutputPolicy,
}

/// Vérifie le résultat publié et, le cas échéant, la sauvegarde de la source.
pub async fn published_source_matches(
    expected: &PublishedSourceExpectation<'_>,
    max_bytes: u64,
) -> AppResult<bool> {
    if !crate::library_scan::valid_relative_name(expected.source_name)
        || !storage::valid_output_extension(expected.extension)
        || expected.policy == LibraryOutputPolicy::OutputDir
    {
        return Ok(false);
    }
    let source = Path::new(expected.source_name);
    let target = source.with_extension(expected.extension);
    let Some(target_name) = target.to_str() else {
        return Ok(false);
    };
    let target_path = crate::library_scan::source_path(expected.root, target_name).await?;
    if verify_source(
        &target_path,
        expected.result_sha256,
        expected.result_bytes,
        max_bytes,
    )
    .await
    .is_err()
    {
        return Ok(false);
    }
    if target.as_path() != source {
        match tokio::fs::symlink_metadata(expected.root.join(source)).await {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Ok(false),
        }
    }
    if expected.policy == LibraryOutputPolicy::ReplaceWithBackup {
        let mut backup = source.as_os_str().to_owned();
        backup.push(".bak");
        let backup = PathBuf::from(backup);
        let Some(backup_name) = backup.to_str() else {
            return Ok(false);
        };
        let backup_path = crate::library_scan::source_path(expected.root, backup_name).await?;
        if verify_source(
            &backup_path,
            expected.source_sha256,
            expected.source_bytes,
            max_bytes,
        )
        .await
        .is_err()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

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
    if !storage::valid_output_extension(extension) {
        return Err(AppError::Invalid);
    }
    let target = source.with_extension(extension);
    let temp = source.with_file_name(format!(".forgearr-replace-{}.tmp", Uuid::new_v4()));
    let mut created_backup: Option<PathBuf> = None;
    let mut created_target = false;
    let mut source_replaced = false;
    let result = async {
        verify_source(source, expected_hash, expected_bytes, max_bytes).await?;
        let metadata = tokio::fs::metadata(source).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::SourceChanged
            } else {
                AppError::Storage(error)
            }
        })?;
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
        verify_source(source, expected_hash, expected_bytes, max_bytes).await?;
        authorize.await?;
        // L'autorisation peut nécessiter un aller-retour réseau : contrôler de nouveau
        // la source avant toute modification du système de fichiers.
        verify_source(source, expected_hash, expected_bytes, max_bytes).await?;
        let verified_at = SystemTime::now();
        if policy == LibraryOutputPolicy::ReplaceWithBackup {
            let mut backup = source.as_os_str().to_owned();
            backup.push(".bak");
            let backup = PathBuf::from(backup);
            tokio::fs::hard_link(source, &backup).await?;
            created_backup = Some(backup);
        }
        if verification_stale(verified_at) {
            verify_source(source, expected_hash, expected_bytes, max_bytes).await?;
        }
        if target == source {
            tokio::fs::rename(&temp, &target).await?;
        } else {
            tokio::fs::hard_link(&temp, &target).await?;
            created_target = true;
            tokio::fs::remove_file(source).await?;
        }
        source_replaced = true;
        let parent = source.parent().ok_or(AppError::Invalid)?;
        tokio::fs::File::open(parent).await?.sync_all().await?;
        Ok(())
    }
    .await;
    if result.is_err() && !source_replaced {
        if created_target {
            let _ = tokio::fs::remove_file(&target).await;
        }
        if let Some(backup) = created_backup {
            let _ = tokio::fs::remove_file(backup).await;
        }
    }
    let _ = tokio::fs::remove_file(temp).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_BYTES: u64 = 1024;

    #[test]
    fn an_old_verification_or_a_clock_jump_is_not_trusted() {
        let now = SystemTime::now();
        assert!(!verification_stale(now));
        assert!(verification_stale(
            now - VERIFICATION_MAX_AGE - Duration::from_secs(1)
        ));
        assert!(verification_stale(now + Duration::from_secs(3600)));
    }

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
            Err(AppError::SourceChanged)
        ));
        assert_eq!(std::fs::read(&source)?, b"modified");
        assert!(!directory.path().join("film.mkv.bak").exists());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_path_like_output_extensions_before_touching_source() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("film.mkv");
        let output = directory.path().join("result.mkv");
        std::fs::write(&source, b"original")?;
        std::fs::write(&output, b"result")?;
        let (hash, bytes) = storage::fingerprint(&source, MAX_BYTES).await?;
        assert!(matches!(
            apply_library_output_policy(
                LibraryOutputPolicy::ReplaceWithBackup,
                &source,
                &output,
                "../outside",
                &hash,
                bytes as i64,
                MAX_BYTES,
                async { Ok(()) },
            )
            .await,
            Err(AppError::Invalid)
        ));
        assert_eq!(std::fs::read(&source)?, b"original");
        assert!(!directory.path().join("film.mkv.bak").exists());
        Ok(())
    }

    #[tokio::test]
    async fn inspection_requires_matching_result_and_backup_without_writing() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(dir.path().join("film.mkv"), b"original").await?;
        let (source_hash, source_bytes) =
            storage::fingerprint(&dir.path().join("film.mkv"), MAX_BYTES).await?;
        tokio::fs::rename(dir.path().join("film.mkv"), dir.path().join("film.mkv.bak")).await?;
        tokio::fs::write(dir.path().join("film.mp4"), b"result").await?;
        let (result_hash, result_bytes) =
            storage::fingerprint(&dir.path().join("film.mp4"), MAX_BYTES).await?;
        let expected = PublishedSourceExpectation {
            root: dir.path(),
            source_name: "film.mkv",
            source_sha256: &source_hash,
            source_bytes: source_bytes as i64,
            result_sha256: &result_hash,
            result_bytes: result_bytes as i64,
            extension: "mp4",
            policy: LibraryOutputPolicy::ReplaceWithBackup,
        };
        assert!(published_source_matches(&expected, MAX_BYTES).await?);
        tokio::fs::write(dir.path().join("film.mkv.bak"), b"wrong").await?;
        assert!(!published_source_matches(&expected, MAX_BYTES).await?);
        assert_eq!(
            tokio::fs::read(dir.path().join("film.mp4")).await?,
            b"result"
        );
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
        assert!(
            apply_library_output_policy(
                LibraryOutputPolicy::ReplaceWithBackup,
                &source,
                &output,
                "mkv",
                &hash,
                bytes as i64,
                MAX_BYTES,
                async { Ok(()) }
            )
            .await
            .is_err()
        );
        assert!(!directory.path().join("film.mp4.bak").exists());
        assert_eq!(std::fs::read(&source)?, b"source");
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

    #[tokio::test]
    async fn refuses_a_source_changed_during_authorization() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("film.mkv");
        let output = directory.path().join("result.mkv");
        std::fs::write(&source, b"source")?;
        std::fs::write(&output, b"result")?;
        let (hash, bytes) = storage::fingerprint(&source, MAX_BYTES).await?;
        let authorization = async {
            tokio::fs::write(&source, b"changed").await?;
            Ok(())
        };
        assert!(matches!(
            apply_library_output_policy(
                LibraryOutputPolicy::ReplaceWithBackup,
                &source,
                &output,
                "mkv",
                &hash,
                bytes as i64,
                MAX_BYTES,
                authorization,
            )
            .await,
            Err(AppError::SourceChanged)
        ));
        assert_eq!(std::fs::read(&source)?, b"changed");
        assert!(!directory.path().join("film.mkv.bak").exists());
        Ok(())
    }

    #[tokio::test]
    async fn missing_empty_and_oversized_sources_are_reported_as_changed() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("film.mkv");
        std::fs::write(&source, b"original")?;
        let (hash, bytes) = storage::fingerprint(&source, MAX_BYTES).await?;
        std::fs::remove_file(&source)?;
        assert!(matches!(
            verify_source(&source, &hash, bytes as i64, MAX_BYTES).await,
            Err(AppError::SourceChanged)
        ));
        std::fs::write(&source, b"")?;
        assert!(matches!(
            verify_source(&source, &hash, bytes as i64, MAX_BYTES).await,
            Err(AppError::SourceChanged)
        ));
        std::fs::write(&source, vec![0; MAX_BYTES as usize + 1])?;
        assert!(matches!(
            verify_source(&source, &hash, bytes as i64, MAX_BYTES).await,
            Err(AppError::SourceChanged)
        ));
        Ok(())
    }
}
