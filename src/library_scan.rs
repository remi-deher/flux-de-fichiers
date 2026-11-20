//! Scan effectué par l'hôte qui monte la source, avant création d'un bail.

use crate::{
    error::{AppError, AppResult},
    storage,
};
use forgearr_shared::libraries::{
    FolderScanFile, FolderScanOptions, FolderScanReport, IgnoredScanFile, ScanIgnoreReason,
    matches_library_filters,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const MIN_STABLE_AGE: Duration = Duration::from_secs(60);
const MAX_RELATIVE_PATH_BYTES: usize = 4096;
const MAX_SCAN_NOTICES_SHOWN: usize = 25;

const SYSTEM_ROOTS: &[&str] = &["/config", "/etc", "/run", "/proc", "/sys", "/dev", "/root"];

pub fn protected_path(path: &Path, private_roots: &[&Path]) -> bool {
    path == Path::new("/")
        || SYSTEM_ROOTS.iter().any(|root| path.starts_with(root))
        || private_roots.iter().any(|root| path.starts_with(root))
}

pub fn valid_relative_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_RELATIVE_PATH_BYTES
        && name.split('/').all(storage::valid_name)
}

pub async fn source_path(root: &Path, name: &str) -> AppResult<PathBuf> {
    if !valid_relative_name(name) {
        return Err(AppError::Invalid);
    }
    let root = tokio::fs::canonicalize(root).await?;
    if protected_path(&root, &[]) {
        return Err(AppError::Invalid);
    }
    let mut path = root;
    let mut segments = name.split('/').peekable();
    while let Some(segment) = segments.next() {
        path.push(segment);
        let kind = tokio::fs::symlink_metadata(&path).await?.file_type();
        if (segments.peek().is_some() && !kind.is_dir())
            || (segments.peek().is_none() && !kind.is_file())
        {
            return Err(AppError::Invalid);
        }
    }
    Ok(path)
}

pub async fn central_job_source(
    pool: &sqlx::SqlitePool,
    fallback: &Path,
    job: &crate::domain::Job,
) -> AppResult<Option<PathBuf>> {
    match crate::repository::job_source(pool, job.id).await? {
        Some(source) if source.source_worker_id.is_some() => Ok(None),
        Some(source) => source_path(Path::new(&source.source_root), &job.source_name)
            .await
            .map(Some),
        None => storage::input_path(fallback, &job.source_name)
            .await
            .map(Some),
    }
}

/// Racine centrale actuellement inscriptible pour la source de la tâche, si elle existe.
pub async fn central_writable_job_source(
    pool: &sqlx::SqlitePool,
    fallback: &Path,
    job: &crate::domain::Job,
) -> AppResult<Option<PathBuf>> {
    match central_writable_job_root(pool, fallback, job).await? {
        Some(root) => source_path(&root, &job.source_name).await.map(Some),
        None => Ok(None),
    }
}

/// Racine centrale vérifiée même si le nom d'origine a disparu après un remplacement.
pub async fn central_writable_job_root(
    pool: &sqlx::SqlitePool,
    fallback: &Path,
    job: &crate::domain::Job,
) -> AppResult<Option<PathBuf>> {
    let source = crate::repository::job_source(pool, job.id).await?;
    match source {
        Some(source) if source.source_key.is_some() => {
            let root: Option<String> = sqlx::query_scalar(
                "SELECT local_path FROM verified_job_source_mounts WHERE job_id=?1 AND worker_id=?2 AND writable=1 LIMIT 1",
            )
            .bind(job.id)
            .bind(uuid::Uuid::nil())
            .fetch_optional(pool)
            .await?;
            Ok(root.map(PathBuf::from))
        }
        Some(source) if source.source_worker_id.is_none() && source.source_writable => {
            Ok(Some(PathBuf::from(source.source_root)))
        }
        Some(_) => Ok(None),
        None => Ok(Some(fallback.to_path_buf())),
    }
}

pub async fn scan_folder(
    root: &Path,
    options: &FolderScanOptions,
    private_roots: &[&Path],
) -> AppResult<FolderScanReport> {
    let root = tokio::fs::canonicalize(root).await?;
    let mut restricted = Vec::with_capacity(private_roots.len());
    for path in private_roots {
        if let Ok(path) = tokio::fs::canonicalize(path).await {
            restricted.push(path);
        }
    }
    let private: Vec<&Path> = restricted.iter().map(PathBuf::as_path).collect();
    if protected_path(&root, &private) {
        return Err(AppError::Invalid);
    }
    let mut directories = vec![root.to_path_buf()];
    let mut report = FolderScanReport::default();
    let now = SystemTime::now();
    while let Some(path) = directories.pop() {
        let mut entries = tokio::fs::read_dir(path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(name) = path.strip_prefix(&root).ok().and_then(Path::to_str) else {
                continue;
            };
            if !valid_relative_name(name) || protected_path(&path, &private) {
                continue;
            }
            let kind = entry.file_type().await?;
            if kind.is_dir() {
                directories.push(path);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            report.scanned += 1;
            let metadata = entry.metadata().await?;
            let bytes = metadata.len();
            let ignored = if bytes == 0 || bytes > options.max_bytes {
                Some(ScanIgnoreReason::FileSize)
            } else if !matches_library_filters(
                &options.library.include_extensions,
                &options.library.exclude_patterns,
                options.library.min_file_bytes,
                name,
                bytes,
            ) {
                Some(ScanIgnoreReason::LibraryFilter)
            } else {
                None
            };
            if let Some(reason) = ignored {
                if report.ignored_files.len() < MAX_SCAN_NOTICES_SHOWN {
                    report.ignored_files.push(IgnoredScanFile {
                        path: name.to_owned(),
                        reason,
                    });
                }
                continue;
            }
            let modified = metadata.modified()?;
            let mtime = modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_nanos()).ok());
            if mtime.is_some_and(|time| options.known_files.get(name) == Some(&(bytes, time))) {
                report.skipped_known += 1;
                continue;
            }
            if now.duration_since(modified).unwrap_or_default() < MIN_STABLE_AGE {
                report.waiting_for_stability += 1;
                if report.pending_files.len() < MAX_SCAN_NOTICES_SHOWN {
                    report.pending_files.push(name.to_owned());
                }
                continue;
            }
            let (sha256, size) = storage::fingerprint(&path, options.max_bytes).await?;
            let after = tokio::fs::symlink_metadata(&path).await?;
            if !after.is_file()
                || after.len() != bytes
                || size != bytes
                || after.modified()? != modified
            {
                report.waiting_for_stability += 1;
                if report.pending_files.len() < MAX_SCAN_NOTICES_SHOWN {
                    report.pending_files.push(name.to_owned());
                }
                continue;
            }
            report.files.push(FolderScanFile {
                name: name.to_owned(),
                bytes,
                sha256,
                mtime_ns: mtime,
            });
        }
    }
    report.files.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scans_nested_paths_and_waits_for_recent_files() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let nested = directory.path().join("saison");
        std::fs::create_dir(&nested)?;
        let ready = std::fs::File::create(nested.join("episode.mkv"))?;
        use std::io::Write;
        (&ready).write_all(b"old")?;
        ready.set_modified(SystemTime::now() - MIN_STABLE_AGE - Duration::from_secs(1))?;
        std::fs::write(directory.path().join("arriving.mkv"), b"new")?;
        std::os::unix::fs::symlink("saison", directory.path().join("shortcut"))?;
        let report = scan_folder(
            directory.path(),
            &FolderScanOptions {
                library: Default::default(),
                max_bytes: 10,
                known_files: Default::default(),
            },
            &[],
        )
        .await?;
        assert_eq!(report.scanned, 2);
        assert_eq!(report.waiting_for_stability, 1);
        assert_eq!(report.files[0].name, "saison/episode.mkv");
        assert!(
            source_path(directory.path(), "shortcut/episode.mkv")
                .await
                .is_err()
        );
        assert!(!valid_relative_name("saison/../episode.mkv"));
        Ok(())
    }
}
