//! Exploration du système de fichiers (dossiers et médias) local et relayée vers un worker distant.

use crate::{
    domain::{
        DirectoryEntry, DirectoryListing, LIBRARY_PATH_MAX, MediaFileEntry, RootShortcut,
        WorkerBrowseReport, WorkerBrowseTask,
    },
    error::{AppError, AppResult},
};
use forgearr_shared::libraries::is_media_extension;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, Notify, oneshot};
use uuid::Uuid;

pub const MAX_BROWSE_ENTRIES: usize = 300;
pub const BROWSE_REQUEST_TTL: Duration = Duration::from_secs(30);
pub const SYSTEM_ROOT_EXCLUDED_DIRS: &[&str] = &[
    "proc",
    "sys",
    "dev",
    "run",
    "boot",
    "etc",
    "usr",
    "bin",
    "sbin",
    "lib",
    "lib64",
    "root",
    "lost+found",
];

const DEFAULT_FALLBACK_ROOTS: [&str; 5] = ["/mnt", "/media", "/data", "/input", "/"];

/// Normalise un chemin d'exploration ou choisit un point de montage par défaut si vide.
pub fn normalize_browse_path(raw: Option<&str>, fallback_dir: Option<&Path>) -> AppResult<PathBuf> {
    let trimmed = raw.map(str::trim).filter(|value| !value.is_empty());
    let Some(raw_path) = trimmed else {
        if let Some(dir) = fallback_dir
            && dir.is_absolute()
            && dir.is_dir()
        {
            return Ok(dir.to_path_buf());
        }
        for candidate in DEFAULT_FALLBACK_ROOTS {
            let path = Path::new(candidate);
            if path.is_dir() {
                return Ok(path.to_path_buf());
            }
        }
        return Ok(PathBuf::from("/"));
    };

    if raw_path.chars().any(char::is_control) || raw_path.contains('\\') {
        return Err(AppError::Invalid);
    }
    let mut normalized = raw_path.to_owned();
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    if !normalized.starts_with('/') || normalized.chars().count() > LIBRARY_PATH_MAX {
        return Err(AppError::Invalid);
    }
    if normalized == "/" {
        return Ok(PathBuf::from("/"));
    }
    let mut segments = normalized.split('/');
    if segments.next() != Some("") {
        return Err(AppError::Invalid);
    }
    if segments.any(|segment| segment.is_empty() || segment == "." || segment == "..") {
        return Err(AppError::Invalid);
    }
    Ok(PathBuf::from(normalized))
}

/// Indique si le dossier existe et n'est pas marqué en lecture seule.
pub fn is_dir_writable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_dir() && !metadata.permissions().readonly())
        .unwrap_or(false)
}

/// Détecte les racines et points de montage utiles disponibles sur l'hôte.
pub fn detect_roots(input_dir: Option<&Path>) -> Vec<RootShortcut> {
    let mut roots = Vec::new();
    let mut push_if_dir = |label: &str, path: &Path| {
        if !path.is_absolute() || !path.is_dir() {
            return;
        }
        let path_str = path.to_string_lossy().into_owned();
        if roots
            .iter()
            .any(|existing: &RootShortcut| existing.path == path_str)
        {
            return;
        }
        roots.push(RootShortcut {
            label: label.to_owned(),
            path: path_str,
            writable: is_dir_writable(path),
        });
    };

    if let Some(input) = input_dir {
        push_if_dir("Entrée", input);
    }
    push_if_dir("Montages (/mnt)", Path::new("/mnt"));
    push_if_dir("Médias (/media)", Path::new("/media"));
    push_if_dir("Données (/data)", Path::new("/data"));
    push_if_dir("Services (/srv)", Path::new("/srv"));
    push_if_dir("Racine (/)", Path::new("/"));
    roots
}

async fn summarize_subdir(path: &Path) -> (bool, usize) {
    let Ok(mut entries) = tokio::fs::read_dir(path).await else {
        return (false, 0);
    };
    let mut has_subdirs = false;
    let mut media_count = 0usize;
    let mut inspected = 0usize;
    while let Ok(Some(entry)) = entries.next_entry().await {
        inspected += 1;
        if inspected > MAX_BROWSE_ENTRIES {
            break;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let entry_path = entry.path();
        let Ok(meta) = tokio::fs::metadata(&entry_path).await else {
            continue;
        };
        if meta.is_dir() {
            has_subdirs = true;
        } else if meta.is_file()
            && entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(is_media_extension)
        {
            media_count += 1;
        }
    }
    (has_subdirs, media_count)
}

fn compare_names(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

/// Explore localement les sous-dossiers et fichiers médias d'un dossier donné.
pub async fn browse_local(
    worker_id: Uuid,
    raw_path: Option<&str>,
    input_dir: Option<&Path>,
) -> AppResult<DirectoryListing> {
    let target = normalize_browse_path(raw_path, input_dir)?;
    let metadata = tokio::fs::metadata(&target).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound
        } else {
            AppError::Storage(err)
        }
    })?;
    if !metadata.is_dir() {
        return Err(AppError::Invalid);
    }

    let is_root = target == Path::new("/");
    let mut entries = tokio::fs::read_dir(&target).await?;
    let mut directories = Vec::new();
    let mut files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        if directories.len() + files.len() >= MAX_BROWSE_ENTRIES {
            break;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if is_root && SYSTEM_ROOT_EXCLUDED_DIRS.contains(&name) {
            continue;
        }
        let entry_path = entry.path();
        let Ok(entry_meta) = tokio::fs::metadata(&entry_path).await else {
            continue;
        };
        if entry_meta.is_dir() {
            let (has_subdirs, media_count) = summarize_subdir(&entry_path).await;
            directories.push(DirectoryEntry {
                name: name.to_owned(),
                path: entry_path.to_string_lossy().into_owned(),
                writable: !entry_meta.permissions().readonly(),
                has_subdirs,
                media_count,
            });
        } else if entry_meta.is_file()
            && let Some(ext) = entry_path.extension().and_then(|value| value.to_str())
            && is_media_extension(ext)
        {
            files.push(MediaFileEntry {
                name: name.to_owned(),
                path: entry_path.to_string_lossy().into_owned(),
                bytes: entry_meta.len(),
                extension: ext.to_ascii_lowercase(),
            });
        }
    }

    directories.sort_by(|a, b| compare_names(&a.name, &b.name));
    files.sort_by(|a, b| compare_names(&a.name, &b.name));

    let parent_path = if is_root {
        None
    } else {
        target
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
    };

    Ok(DirectoryListing {
        worker_id,
        path: target.to_string_lossy().into_owned(),
        parent_path,
        writable: !metadata.permissions().readonly(),
        roots: detect_roots(input_dir),
        directories,
        files,
    })
}

/// Code texte renvoyé au central lorsqu'une exploration distante échoue.
pub fn error_code_for_report(err: &AppError) -> &'static str {
    match err {
        AppError::NotFound => "not_found",
        AppError::Invalid => "invalid_input",
        _ => "unavailable",
    }
}

struct PendingRequest {
    task: WorkerBrowseTask,
    dispatched: bool,
    created_at: tokio::time::Instant,
    reply: oneshot::Sender<Result<DirectoryListing, String>>,
}

#[derive(Default)]
struct BrokerInner {
    state: Mutex<HashMap<Uuid, PendingRequest>>,
    notify: Notify,
}

/// Courtier en mémoire qui relaie les demandes d'exploration vers un worker distant connecté.
#[derive(Clone, Default)]
pub struct RemoteBrowseBroker {
    inner: Arc<BrokerInner>,
}

impl RemoteBrowseBroker {
    pub fn new() -> Self {
        Self::default()
    }

    fn purge_expired(state: &mut HashMap<Uuid, PendingRequest>) {
        let now = tokio::time::Instant::now();
        state.retain(|_, req| now.saturating_duration_since(req.created_at) < BROWSE_REQUEST_TTL);
    }

    /// Met une demande d'exploration en file pour `worker_id` et attend la réponse jusqu'à `timeout`.
    pub async fn request(
        &self,
        worker_id: Uuid,
        path: Option<String>,
        timeout: Duration,
    ) -> AppResult<DirectoryListing> {
        let (tx, rx) = oneshot::channel();
        let request_id = Uuid::new_v4();
        {
            let mut state = self.inner.state.lock().await;
            Self::purge_expired(&mut state);
            state.insert(
                request_id,
                PendingRequest {
                    task: WorkerBrowseTask {
                        request_id,
                        worker_id,
                        path,
                    },
                    dispatched: false,
                    created_at: tokio::time::Instant::now(),
                    reply: tx,
                },
            );
        }
        self.inner.notify.notify_waiters();

        let awaited = tokio::time::timeout(timeout, rx).await;
        match awaited {
            Err(_) => {
                let mut state = self.inner.state.lock().await;
                state.remove(&request_id);
                Err(AppError::CentralUnavailable)
            }
            Ok(Err(_)) => Err(AppError::CentralUnavailable),
            Ok(Ok(Ok(listing))) => Ok(listing),
            Ok(Ok(Err(code))) => match code.as_str() {
                "not_found" => Err(AppError::NotFound),
                "invalid_input" => Err(AppError::Invalid),
                _ => Err(AppError::CentralUnavailable),
            },
        }
    }

    async fn drain_pending(&self, worker_id: Uuid) -> Vec<WorkerBrowseTask> {
        let mut state = self.inner.state.lock().await;
        Self::purge_expired(&mut state);
        let mut tasks = Vec::new();
        for entry in state.values_mut() {
            if entry.task.worker_id == worker_id && !entry.dispatched {
                entry.dispatched = true;
                tasks.push(entry.task.clone());
            }
        }
        tasks
    }

    /// Récupère les demandes en attente pour un worker, avec attente longue optionnelle.
    pub async fn take_for_worker(&self, worker_id: Uuid, wait: Duration) -> Vec<WorkerBrowseTask> {
        let tasks = self.drain_pending(worker_id).await;
        if !tasks.is_empty() || wait.is_zero() {
            return tasks;
        }
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let notified = self.inner.notify.notified();
            let tasks = self.drain_pending(worker_id).await;
            if !tasks.is_empty() {
                return tasks;
            }
            if tokio::time::Instant::now() >= deadline {
                return Vec::new();
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.drain_pending(worker_id).await;
            }
        }
    }

    /// Délivre la réponse du worker à la requête HTTP en attente.
    pub async fn complete(&self, report: WorkerBrowseReport) -> bool {
        let entry = {
            let mut state = self.inner.state.lock().await;
            let matches_worker = state
                .get(&report.request_id)
                .is_some_and(|pending| pending.task.worker_id == report.worker_id);
            if !matches_worker {
                return false;
            }
            state.remove(&report.request_id)
        };
        let Some(entry) = entry else {
            return false;
        };
        let outcome = match report.listing {
            Some(listing) => Ok(listing),
            None => Err(report.error.unwrap_or_else(|| "unavailable".to_owned())),
        };
        let _ = entry.reply.send(outcome);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_browse_path_rejects_traversal_and_invalid_chars() -> anyhow::Result<()> {
        assert_eq!(normalize_browse_path(Some("/"), None)?, PathBuf::from("/"));
        assert_eq!(
            normalize_browse_path(Some(" /mnt/films/ "), None)?,
            PathBuf::from("/mnt/films")
        );
        assert!(normalize_browse_path(Some("relative/path"), None).is_err());
        assert!(normalize_browse_path(Some("/mnt/../etc"), None).is_err());
        assert!(normalize_browse_path(Some("/mnt/./films"), None).is_err());
        assert!(normalize_browse_path(Some("/mnt//films"), None).is_err());
        assert!(normalize_browse_path(Some("/mnt\\films"), None).is_err());
        assert!(normalize_browse_path(Some("/mnt/fi\nlms"), None).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn browse_local_lists_subdirectories_media_files_and_readonly_flag() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let films = root.join("Films");
        let empty = root.join("Vide");
        let nested = films.join("4K");
        let hidden = root.join(".cache");
        tokio::fs::create_dir_all(&nested).await?;
        tokio::fs::create_dir_all(&empty).await?;
        tokio::fs::create_dir_all(&hidden).await?;
        tokio::fs::write(films.join("movie.mkv"), b"video-bytes").await?;
        tokio::fs::write(films.join("poster.jpg"), b"image").await?;
        tokio::fs::write(root.join("trailer.MP4"), b"trailer").await?;
        tokio::fs::write(root.join("readme.txt"), b"ignored").await?;
        tokio::fs::write(root.join(".hidden.mkv"), b"hidden").await?;

        let worker_id = Uuid::new_v4();
        let listing =
            browse_local(worker_id, Some(root.to_string_lossy().as_ref()), Some(root)).await?;

        assert_eq!(listing.worker_id, worker_id);
        assert!(listing.writable);
        assert_eq!(listing.directories.len(), 2);
        assert_eq!(listing.directories[0].name, "Films");
        assert!(listing.directories[0].has_subdirs);
        assert_eq!(listing.directories[0].media_count, 1);
        assert_eq!(listing.directories[1].name, "Vide");
        assert!(!listing.directories[1].has_subdirs);
        assert_eq!(listing.directories[1].media_count, 0);

        assert_eq!(listing.files.len(), 1);
        assert_eq!(listing.files[0].name, "trailer.MP4");
        assert_eq!(listing.files[0].extension, "mp4");
        assert_eq!(listing.files[0].bytes, 7);

        let mut perms = std::fs::metadata(&empty)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&empty, perms)?;
        let ro_listing =
            browse_local(worker_id, Some(empty.to_string_lossy().as_ref()), None).await?;
        assert!(!ro_listing.writable);

        let mut rw_perms = std::fs::metadata(&empty)?.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        rw_perms.set_readonly(false);
        std::fs::set_permissions(&empty, rw_perms)?;
        Ok(())
    }

    #[tokio::test]
    async fn remote_browse_broker_roundtrips_request_take_and_complete() -> anyhow::Result<()> {
        let broker = RemoteBrowseBroker::new();
        let worker_id = Uuid::new_v4();
        let broker_worker = broker.clone();

        let responder = tokio::spawn(async move {
            let tasks = broker_worker
                .take_for_worker(worker_id, Duration::from_secs(2))
                .await;
            assert_eq!(tasks.len(), 1);
            let task = &tasks[0];
            assert_eq!(task.path.as_deref(), Some("/mnt/medias"));
            let accepted = broker_worker
                .complete(WorkerBrowseReport {
                    request_id: task.request_id,
                    worker_id,
                    listing: Some(DirectoryListing {
                        worker_id,
                        path: "/mnt/medias".into(),
                        parent_path: Some("/mnt".into()),
                        writable: true,
                        roots: Vec::new(),
                        directories: Vec::new(),
                        files: Vec::new(),
                    }),
                    error: None,
                })
                .await;
            assert!(accepted);
        });

        let result = broker
            .request(
                worker_id,
                Some("/mnt/medias".into()),
                Duration::from_secs(2),
            )
            .await?;
        assert_eq!(result.path, "/mnt/medias");
        responder.await?;
        Ok(())
    }

    #[tokio::test]
    async fn remote_browse_broker_rejects_mismatched_worker_and_maps_errors() {
        let broker = RemoteBrowseBroker::new();
        let worker_a = Uuid::new_v4();
        let worker_b = Uuid::new_v4();
        let broker_worker = broker.clone();

        let responder = tokio::spawn(async move {
            let tasks = broker_worker
                .take_for_worker(worker_a, Duration::from_secs(2))
                .await;
            assert_eq!(tasks.len(), 1);
            let task = &tasks[0];
            assert!(
                !broker_worker
                    .complete(WorkerBrowseReport {
                        request_id: task.request_id,
                        worker_id: worker_b,
                        listing: None,
                        error: Some("not_found".into()),
                    })
                    .await
            );
            assert!(
                broker_worker
                    .complete(WorkerBrowseReport {
                        request_id: task.request_id,
                        worker_id: worker_a,
                        listing: None,
                        error: Some("not_found".into()),
                    })
                    .await
            );
        });

        let err = broker
            .request(worker_a, Some("/absent".into()), Duration::from_secs(2))
            .await;
        assert!(matches!(err, Err(AppError::NotFound)));
        let _ = responder.await;
    }
}
