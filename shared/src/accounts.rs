//! Contrat des comptes, des clés d'API et de la connexion (ADR 0011, 0014).

use crate::Role;
use serde::{Deserialize, Serialize};

/// Un compte humain. Le haché du mot de passe n'en fait jamais partie.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
    /// Adresse de réinitialisation du mot de passe, absente tant qu'elle n'est pas renseignée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Clé d'API telle que listée : jamais son secret ni son haché.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAccountRequest {
    pub role: Option<String>,
    pub disabled: Option<bool>,
    /// Nouvelle adresse ; une chaîne vide l'efface.
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub role: String,
}

/// Clé créée : `key` n'est renvoyée qu'ici.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatedKey {
    pub key: String,
    #[serde(flatten)]
    pub record: ApiKeyRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Demande de lien de réinitialisation : la réponse est la même quelle que soit l'adresse.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

/// Nouveau mot de passe choisi avec le jeton reçu par courriel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}
