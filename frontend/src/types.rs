//! Modèles de données pour le poste de pilotage Flux de Fichiers.

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
    pub worker_id: Option<String>,
    pub status: String,
    pub attempts: i32,
    pub output_name: Option<String>,
    pub error_code: Option<String>,
    pub warning: Option<bool>,
    pub created_at: String,
    pub updated_at: Option<String>,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionInfo {
    pub authenticated: bool,
    pub auth_mode: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ToolManifestSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub capability: String,
    pub current_choice: bool,
    pub distribution_mode: String,
    pub cache_status: String, // "ready", "lock", "not_installed"
    pub architectures: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct HardwareTelemetrySummary {
    pub cpu_usage_percent: f32,
    pub cpu_logical_cores: u32,
    pub cpu_load_avg: [f32; 3],
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub gpu_device: Option<String>,
    pub storage_free_bytes: u64,
    pub storage_total_bytes: u64,
}
