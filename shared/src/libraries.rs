//! Contrat des bibliothèques : un nom, un flux, un chemin par worker (ADR 0014).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LIBRARY_NAME_MAX: usize = 80;
pub const LIBRARY_PATH_MAX: usize = 512;

/// Accès d'un worker au dossier de la bibliothèque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum LibraryAccess {
    Read,
    ReadWrite,
}

/// Chemin local d'une bibliothèque sur un worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryWorkerPath {
    pub worker_id: Uuid,
    pub local_path: String,
    pub access: LibraryAccess,
}

/// Bibliothèque déclarée : un nom, un flux, des workers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    pub flow_id: Option<String>,
    pub workers: Vec<LibraryWorkerPath>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Création ou remplacement d'une bibliothèque.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryWrite {
    pub name: String,
    #[serde(default)]
    pub flow_id: Option<String>,
    #[serde(default)]
    pub workers: Vec<LibraryWorkerPath>,
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
}

/// Réponse d'exploration renvoyée au central par un worker distant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerBrowseReport {
    pub request_id: Uuid,
    pub worker_id: Uuid,
    pub listing: Option<DirectoryListing>,
    pub error: Option<String>,
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg", "ogv",
    "flac", "mp3", "aac", "m4a", "ogg", "opus", "wav", "srt", "ass", "ssa", "vtt", "sub", "sup",
];

const GENERIC_MOUNT_NAMES: &[&str] = &[
    "mnt", "media", "medias", "data", "input", "srv", "volumes", "var", "tmp", "home",
];

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
}
