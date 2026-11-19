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

/// Demande d'enrôlement d'une installation vierge par approbation sur le central (ADR 0016).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupWorkerApprovalRequest {
    pub central_url: String,
    #[serde(default)]
    pub worker_name: String,
}

/// État courant d'une demande d'enrôlement par approbation côté installation vierge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerSetupRequestState {
    #[default]
    Idle,
    Pending,
    Approved,
    Rejected,
    Expired,
    Error,
}

/// Statut renvoyé au navigateur pendant l'attente d'approbation sur le central.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupWorkerRequestStatus {
    pub state: WorkerSetupRequestState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub central_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
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

/// Résultat de la vérification facultative des mises à jour publiées sur GitHub.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckStatus {
    pub enabled: bool,
    pub current_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    pub update_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    /// Version de la release exécutée.
    pub version: String,
    /// Version compilée dans le binaire : témoin de cohérence avec l'interface WASM.
    #[serde(default)]
    pub build_version: String,
    pub key_id: String,
    /// Anciennes clés gardées pour déchiffrer seulement : non vide pendant une rotation.
    #[serde(default)]
    pub retired_key_ids: Vec<String>,
    /// Clé active au moment du dernier kit de récupération exporté ; `None` si jamais exporté.
    #[serde(default)]
    pub recovery_kit_key_id: Option<String>,
    pub key_source: KeySource,
    /// Taille de la base SQLite et de son journal ; `None` pour une base en mémoire.
    pub database_bytes: Option<u64>,
    pub network: NetworkStatus,
    pub storage: StorageStatus,
    #[serde(default)]
    pub smtp: SmtpStatus,
    #[serde(default)]
    pub outbound: OutboundPolicyStatus,
    #[serde(default)]
    pub check_updates_enabled: bool,
    /// Fuseau horaire IANA actif sur le central (`Europe/Paris`, `UTC`, `Etc/GMT`…).
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    crate::timezone::DEFAULT_TIMEZONE.to_owned()
}

impl SystemStatus {
    /// Version compilée du serveur ; `version` pour un serveur qui ne la renseigne pas.
    pub fn build(&self) -> &str {
        if self.build_version.is_empty() {
            &self.version
        } else {
            &self.build_version
        }
    }
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
    #[serde(default)]
    pub check_updates_enabled: bool,
    /// Fuseau horaire IANA optionnel (`None` conserve la valeur actuelle).
    #[serde(default)]
    pub timezone: Option<String>,
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

/// Longueur minimale, en caractères, de la phrase de passe d'un kit de récupération.
pub const RECOVERY_PASSPHRASE_MIN_LEN: usize = 12;
/// Valeur du champ `format` d'un kit de récupération.
pub const RECOVERY_KIT_FORMAT: &str = "flux-recovery-kit";

/// Fonction de dérivation de la phrase de passe d'un kit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKdfAlgorithm {
    Argon2id,
}

/// Paramètres de dérivation inscrits dans le kit, pour qu'il reste lisible si les réglages changent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKdf {
    pub algorithm: RecoveryKdfAlgorithm,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    /// Sel aléatoire, en hexadécimal.
    pub salt: String,
}

/// Kit de récupération : le trousseau de clés chiffré par une phrase de passe choisie par
/// l'administrateur. Sans lui, une base restaurée sans `/config/secret_key` perd ses secrets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKit {
    pub format: String,
    pub version: u32,
    /// Clé active au moment de l'export.
    pub key_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub kdf: RecoveryKdf,
    /// Nonce puis texte chiffré, en hexadécimal.
    pub nonce: String,
    pub ciphertext: String,
}

/// Corps de `POST /api/v1/system/secret-key/recovery-kit` : la phrase de passe du kit à créer.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecoveryKitRequest {
    pub passphrase: String,
}

/// Corps de `POST /api/v1/system/secret-key/restore` : un kit et sa phrase de passe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRecoveryKitRequest {
    pub kit: RecoveryKit,
    pub passphrase: String,
}
