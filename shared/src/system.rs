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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub version: String,
    pub key_id: String,
    pub key_source: KeySource,
    /// Taille de la base SQLite et de son journal ; `None` pour une base en mémoire.
    pub database_bytes: Option<u64>,
}
