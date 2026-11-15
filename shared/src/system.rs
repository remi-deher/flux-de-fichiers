//! Contrat de l'installation et de l'état système (ADR 0011, 0014).

use crate::InstanceKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub needs_setup: bool,
    pub instance_name: Option<String>,
    pub instance_kind: InstanceKind,
    /// Le central peut-il envoyer un lien de réinitialisation par courriel ?
    #[serde(default)]
    pub password_reset_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetupRequest {
    pub name: String,
    pub username: String,
    pub password: String,
    /// Adresse de l'administrateur, pour le mot de passe oublié (facultative).
    #[serde(default)]
    pub email: Option<String>,
}

/// L'installation est-elle à faire ? Public : l'interface choisit entre assistant et connexion.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetupWorkerRequest {
    pub central_url: String,
    pub username: String,
    pub password: String,
    pub worker_name: String,
}

/// Origine de la clé de chiffrement : fichier local de `/config`, ou fichier fourni par l'orchestrateur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    Local,
    External,
}

/// Réseau du central : écoute, TLS direct, origine web et durée de session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub bind: String,
    /// Le central sert-il lui-même le TLS (`TLS_CERT` / `TLS_KEY`) ?
    pub tls: bool,
    /// Origine web autorisée pour les sessions ; `None` : celle de l'adresse d'accès.
    pub web_origin: Option<String>,
    pub session_ttl_seconds: i64,
}

/// Dossiers du central, taille maximale d'un fichier et espace libre du volume de travail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    pub config_dir: String,
    pub data_dir: String,
    pub input_dir: String,
    pub tools_cache_dir: String,
    pub plugins_dir: Option<String>,
    pub max_file_bytes: u64,
    /// Capacité et espace disponible du volume de `data_dir` ; `None` si illisibles.
    pub volume: Option<VolumeSpace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpace {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// Mode de chiffrement SMTP supporté par le central.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTls {
    /// Connexion en clair puis passage en TLS (port 587 par défaut).
    #[default]
    #[serde(rename = "starttls")]
    StartTls,
    /// TLS dès la connexion (port 465 par défaut).
    Tls,
    /// Aucun chiffrement (port 25 par défaut) : à réserver à un relais local.
    None,
}

impl SmtpTls {
    pub const SMTP_STARTTLS_PORT: u16 = 587;
    pub const SMTP_TLS_PORT: u16 = 465;
    pub const SMTP_PLAIN_PORT: u16 = 25;

    pub const fn default_port(self) -> u16 {
        match self {
            Self::StartTls => Self::SMTP_STARTTLS_PORT,
            Self::Tls => Self::SMTP_TLS_PORT,
            Self::None => Self::SMTP_PLAIN_PORT,
        }
    }
}

/// État public de la configuration SMTP (le mot de passe n'est jamais exposé).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtpStatus {
    pub configured: bool,
    pub host: Option<String>,
    pub port: u16,
    pub tls: SmtpTls,
    pub username: Option<String>,
    pub from: Option<String>,
    pub password_set: bool,
}

/// Politique de requêtes sortantes appliquée aux dépôts et intégrations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundPolicyStatus {
    pub allow_private_plugin_hosts: bool,
    pub block_private_integration_hosts: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub version: String,
    pub key_id: String,
    pub key_source: KeySource,
    /// Taille de la base SQLite et de son journal ; `None` pour une base en mémoire.
    pub database_bytes: Option<u64>,
    pub network: NetworkStatus,
    pub storage: StorageStatus,
    #[serde(default)]
    pub smtp: SmtpStatus,
    #[serde(default)]
    pub outbound: OutboundPolicyStatus,
}

/// Corps de mise à jour des réglages réseau, sessions, SMTP et politique sortante (`PUT /api/v1/system/settings`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSystemSettingsRequest {
    pub web_origin: Option<String>,
    pub session_ttl_seconds: i64,
    pub smtp_enabled: bool,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_tls: SmtpTls,
    pub smtp_username: Option<String>,
    /// Laisser `None` pour conserver le mot de passe actuel ; `Some("")` efface le mot de passe avec le compte.
    #[serde(default)]
    pub smtp_password: Option<String>,
    pub smtp_from: Option<String>,
    pub allow_private_plugin_hosts: bool,
    pub block_private_integration_hosts: bool,
}

/// Charge matérielle du central, relevée à la demande (`GET /api/v1/system/hardware`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareStatus {
    pub logical_cores: u32,
    /// Charge sur 1 minute rapportée aux cœurs, bornée à 0–100.
    pub cpu_usage_percent: f32,
    pub ram_total_bytes: u64,
    pub ram_used_bytes: u64,
    pub gpu_devices: Vec<String>,
    /// Volume de `data_dir` ; `None` si illisible.
    pub volume: Option<VolumeSpace>,
}
