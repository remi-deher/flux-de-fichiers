//! Modèles de données pour l'interface de pilotage Flux de Fichiers.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct JobEntry {
    pub id: String,
    pub source_name: String,
    pub target_worker_id: Option<String>,
    pub status: String,
    pub attempts: i32,
    pub output_name: Option<String>,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkerEntry {
    pub id: String,
    pub name: String,
    pub last_seen: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checked_at: String,
}
