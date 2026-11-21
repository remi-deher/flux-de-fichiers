//! Contrats des bibliothèques, dossiers par hôte, scans et historique.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LIBRARY_NAME_MAX: usize = 80;
pub const LIBRARY_PATH_MAX: usize = 512;
pub const LIBRARY_HISTORY_PAGE_SIZE: usize = 50;
pub const MAX_SCAN_INTERVAL_MINUTES: i32 = 7 * 24 * 60;

/// Stratégie de publication du fichier produit pour une bibliothèque.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum LibraryOutputPolicy {
    /// Place le résultat dans le dossier de sortie dédié (défaut).
    #[default]
    OutputDir,
    /// Remplace la source en conservant une copie `.bak`.
    ReplaceWithBackup,
    /// Remplace directement le fichier source après validation.
    ReplaceInPlace,
}

impl LibraryOutputPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutputDir => "output_dir",
            Self::ReplaceWithBackup => "replace_with_backup",
            Self::ReplaceInPlace => "replace_in_place",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "output_dir" => Some(Self::OutputDir),
            "replace_with_backup" => Some(Self::ReplaceWithBackup),
            "replace_in_place" => Some(Self::ReplaceInPlace),
            _ => None,
        }
    }
}

/// Règles d'automatisation et de filtrage d'une bibliothèque.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAutomation {
    /// `0` = scan manuel uniquement ; sinon intervalle en minutes.
    #[serde(default)]
    pub scan_interval_minutes: i32,
    #[serde(default)]
    pub last_scanned_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub include_extensions: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub min_file_bytes: u64,
    #[serde(default)]
    pub output_policy: LibraryOutputPolicy,
}

/// Accès de l’hôte au dossier de la bibliothèque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum LibraryAccess {
    Read,
    ReadWrite,
}

/// Dossier sur un hôte. Un UUID nul identifie la centrale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryWorkerPath {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    pub worker_id: Uuid,
    pub local_path: String,
    pub access: LibraryAccess,
}

/// Un dossier distant sans identifiant reste attribué à son détenteur, sans transfert.
pub fn has_unshared_remote_sources(folders: &[LibraryWorkerPath]) -> bool {
    folders.iter().any(|folder| {
        !folder.worker_id.is_nil()
            && folder
                .source_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
    })
}

/// Bibliothèque déclarée : un nom, un flux, des workers, et règles d'automatisation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    pub flow_id: Option<String>,
    pub workers: Vec<LibraryWorkerPath>,
    /// Workers sans accès direct qui traitent les tâches par transfert (ADR 0021).
    #[serde(default)]
    pub transfer_workers: Vec<Uuid>,
    #[serde(default)]
    pub scan_interval_minutes: i32,
    #[serde(default)]
    pub last_scanned_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub include_extensions: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub min_file_bytes: u64,
    #[serde(default)]
    pub output_policy: LibraryOutputPolicy,
    /// Priorité des tâches créées sans priorité explicite (`JOB_PRIORITY_MIN..=JOB_PRIORITY_MAX`).
    #[serde(default = "crate::jobs::default_library_job_priority")]
    pub default_priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Création ou remplacement d'une bibliothèque.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryWrite {
    pub name: String,
    #[serde(default)]
    pub flow_id: Option<String>,
    #[serde(default)]
    pub workers: Vec<LibraryWorkerPath>,
    #[serde(default)]
    pub transfer_workers: Vec<Uuid>,
    #[serde(default)]
    pub scan_interval_minutes: i32,
    #[serde(default)]
    pub include_extensions: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub min_file_bytes: u64,
    #[serde(default)]
    pub output_policy: LibraryOutputPolicy,
    /// Priorité des tâches créées sans priorité explicite (`JOB_PRIORITY_MIN..=JOB_PRIORITY_MAX`).
    #[serde(default = "crate::jobs::default_library_job_priority")]
    pub default_priority: i32,
}

/// Raccourci vers une racine ou un point de montage détecté sur un worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootShortcut {
    pub label: String,
    pub path: String,
    pub writable: bool,
}

/// Sous-dossier retourné lors de l'exploration d'un worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub writable: bool,
    pub has_subdirs: bool,
    pub media_count: usize,
}

/// Fichier média présent dans le dossier inspecté.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFileEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub extension: String,
}

/// Contenu d'un dossier sur un worker (racines, sous-dossiers et fichiers médias).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    pub worker_id: Uuid,
    pub path: String,
    pub parent_path: Option<String>,
    pub writable: bool,
    pub roots: Vec<RootShortcut>,
    pub directories: Vec<DirectoryEntry>,
    pub files: Vec<MediaFileEntry>,
}

/// Demande d'exploration de dossier transmise à un worker distant en ligne.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerBrowseTask {
    pub request_id: Uuid,
    pub worker_id: Uuid,
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<FolderScanOptions>,
}

/// Réponse d'exploration renvoyée au central par un worker distant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerBrowseReport {
    pub request_id: Uuid,
    pub worker_id: Uuid,
    pub listing: Option<DirectoryListing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<FolderScanReport>,
    pub error: Option<String>,
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg", "ogv",
    "flac", "mp3", "aac", "m4a", "ogg", "opus", "wav", "srt", "ass", "ssa", "vtt", "sub", "sup",
];

const GENERIC_MOUNT_NAMES: &[&str] = &[
    "mnt", "media", "medias", "data", "input", "srv", "volumes", "var", "tmp", "home",
];

/// Correspondance simple d'un motif avec jokers `*` (insensible à la casse).
pub fn wildcard_matches_ci(pattern: &str, text: &str) -> bool {
    let pat = pattern.trim().to_ascii_lowercase();
    if pat.is_empty() {
        return false;
    }
    let hay = text.to_ascii_lowercase();
    if !pat.contains('*') {
        return hay.contains(&pat);
    }
    let parts: Vec<&str> = pat.split('*').collect();
    let mut cursor = 0_usize;
    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if idx == 0 && !pat.starts_with('*') {
            if !hay[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
        } else if idx == parts.len() - 1 && !pat.ends_with('*') {
            return hay[cursor..].ends_with(part);
        } else if let Some(pos) = hay[cursor..].find(part) {
            cursor += pos + part.len();
        } else {
            return false;
        }
    }
    true
}

/// Vérifie si un fichier `(nom, taille)` satisfait les règles d'inclusion/exclusion d'une bibliothèque.
pub fn library_accepts_file(library: &Library, file_name: &str, bytes: u64) -> bool {
    matches_library_filters(
        &library.include_extensions,
        &library.exclude_patterns,
        library.min_file_bytes,
        file_name,
        bytes,
    )
}

pub fn matches_library_filters(
    include_extensions: &[String],
    exclude_patterns: &[String],
    min_file_bytes: u64,
    file_name: &str,
    bytes: u64,
) -> bool {
    if bytes < min_file_bytes {
        return false;
    }
    if !include_extensions.is_empty() {
        let ext = file_name
            .rsplit('.')
            .next()
            .filter(|_| file_name.contains('.'))
            .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
            .unwrap_or_default();
        let allowed = include_extensions.iter().any(|candidate| {
            candidate
                .trim()
                .trim_start_matches('.')
                .eq_ignore_ascii_case(&ext)
        });
        if !allowed {
            return false;
        }
    }
    !exclude_patterns
        .iter()
        .any(|pat| wildcard_matches_ci(pat, file_name))
}

/// Indique si l'extension (sans le point initial, insensible à la casse) correspond à un média ou sous-titre.
pub fn is_media_extension(ext: &str) -> bool {
    let normalized = ext.trim().to_ascii_lowercase();
    MEDIA_EXTENSIONS.contains(&normalized.as_str())
}

/// Propose un nom de bibliothèque à partir du dernier segment d'un chemin absolu, hors montages génériques.
pub fn suggested_library_name_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let segment = trimmed.split('/').rfind(|part| !part.is_empty())?.trim();
    let count = segment.chars().count();
    if count == 0
        || count > LIBRARY_NAME_MAX
        || segment == "."
        || segment == ".."
        || GENERIC_MOUNT_NAMES.contains(&segment.to_ascii_lowercase().as_str())
    {
        return None;
    }
    Some(segment.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unshared_remote_sources_exclude_central_and_accept_named_sources() {
        let mut folder = LibraryWorkerPath {
            source_key: None,
            worker_id: Uuid::nil(),
            local_path: "/films".into(),
            access: LibraryAccess::Read,
        };
        assert!(!has_unshared_remote_sources(&[folder.clone()]));
        folder.worker_id = Uuid::from_u128(1);
        assert!(has_unshared_remote_sources(&[folder.clone()]));
        folder.source_key = Some("  ".into());
        assert!(has_unshared_remote_sources(&[folder.clone()]));
        folder.source_key = Some("films".into());
        assert!(!has_unshared_remote_sources(&[folder]));
    }

    #[test]
    fn media_extensions_match_case_insensitively() {
        for ext in ["mkv", "MP4", "Flac", "SRT", "m2ts", "ass"] {
            assert!(is_media_extension(ext), "{ext}");
        }
        for ext in ["", "txt", "nfo", "jpg", ".mkv", "exe"] {
            assert!(!is_media_extension(ext), "{ext}");
        }
    }

    #[test]
    fn suggested_library_name_ignores_generic_mounts_and_invalid_paths() {
        assert_eq!(
            suggested_library_name_from_path("/mnt/medias/Films 4K/").as_deref(),
            Some("Films 4K")
        );
        assert_eq!(
            suggested_library_name_from_path(" /data/Séries ").as_deref(),
            Some("Séries")
        );
        assert_eq!(suggested_library_name_from_path("/"), None);
        assert_eq!(suggested_library_name_from_path("/mnt"), None);
        assert_eq!(suggested_library_name_from_path("/mnt/media"), None);
        assert_eq!(suggested_library_name_from_path("films"), None);
        let overlong = format!("/mnt/{}", "a".repeat(LIBRARY_NAME_MAX + 1));
        assert_eq!(suggested_library_name_from_path(&overlong), None);
    }

    #[test]
    fn library_rules_filter_extensions_patterns_and_min_size() {
        let now = Utc::now();
        let lib = Library {
            id: Uuid::nil(),
            name: "Films".into(),
            flow_id: None,
            workers: Vec::new(),
            transfer_workers: Vec::new(),
            scan_interval_minutes: 15,
            last_scanned_at: None,
            include_extensions: vec!["mkv".into(), ".mp4".into()],
            exclude_patterns: vec!["*sample*".into(), "trailer".into()],
            min_file_bytes: 1_000,
            output_policy: LibraryOutputPolicy::OutputDir,
            default_priority: crate::jobs::LIBRARY_DEFAULT_JOB_PRIORITY,
            created_at: now,
            updated_at: now,
        };
        assert!(library_accepts_file(&lib, "Film.2026.MKV", 5_000));
        assert!(library_accepts_file(&lib, "Episode.mp4", 1_000));
        assert!(!library_accepts_file(&lib, "Film.avi", 5_000));
        assert!(!library_accepts_file(&lib, "Film.sample.mkv", 5_000));
        assert!(!library_accepts_file(&lib, "Movie-Trailer.mp4", 5_000));
        assert!(!library_accepts_file(&lib, "Tiny.mkv", 999));
    }
}

/// Options de scan relayées à l'hôte qui monte un dossier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderScanOptions {
    pub library: LibraryAutomation,
    pub max_bytes: u64,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub known_files: std::collections::HashMap<String, (u64, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderScanFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub mtime_ns: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanIgnoreReason {
    FileSize,
    LibraryFilter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredScanFile {
    pub path: String,
    pub reason: ScanIgnoreReason,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderScanReport {
    pub files: Vec<FolderScanFile>,
    pub waiting_for_stability: usize,
    #[serde(default)]
    pub skipped_known: usize,
    #[serde(default)]
    pub pending_files: Vec<String>,
    #[serde(default)]
    pub ignored_files: Vec<IgnoredScanFile>,
    pub scanned: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFileHistory {
    pub source_root: Option<String>,
    pub source_worker_id: Option<Uuid>,
    pub jobs: Vec<crate::jobs::Job>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReprocessLibraryFile {
    pub flow_version: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementStatus {
    Ready,
    Offline,
    OutsideSchedule,
    AtCapacity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPlacement {
    pub worker_id: Uuid,
    pub name: String,
    pub status: PlacementStatus,
    pub gpu_not_reported: bool,
    #[serde(default)]
    pub source_path: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPlacement {
    pub workers: Vec<WorkerPlacement>,
    pub catalog_tools_unavailable: Vec<String>,
}
