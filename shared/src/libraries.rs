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
