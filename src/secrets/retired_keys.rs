//! Clés retirées du trousseau : elles ne chiffrent plus rien mais déchiffrent encore les valeurs
//! qui portent leur identifiant. Une clé hexadécimale par ligne dans `<config>/secret_keys.retired`
//! (lecture et écriture propriétaire seules).

use super::KeyEntry;
use crate::error::{AppError, AppResult};
use std::{fs, io::ErrorKind, path::Path};

const RETIRED_KEYS_FILE_NAME: &str = "secret_keys.retired";

/// Fichier des clés retirées dans le dossier de configuration.
pub fn retired_keys_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join(RETIRED_KEYS_FILE_NAME)
}

pub(super) fn read_lines(config_dir: &Path) -> AppResult<Vec<String>> {
    match fs::read_to_string(retired_keys_path(config_dir)) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(AppError::Storage(error)),
    }
}

/// Clés retirées lisibles ; une ligne illisible rend le trousseau inutilisable plutôt que d'être ignorée.
pub(super) fn load(config_dir: &Path) -> AppResult<Vec<KeyEntry>> {
    read_lines(config_dir)?
        .iter()
        .map(|line| KeyEntry::from_hex(line))
        .collect()
}

/// Oublie toutes les clés retirées. À n'appeler qu'une fois chaque valeur rechiffrée sous la clé active.
pub fn clear_retired_keys(config_dir: &Path) -> AppResult<()> {
    match fs::remove_file(retired_keys_path(config_dir)) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(AppError::Storage(error)),
        _ => Ok(()),
    }
}

/// Ajoute une clé au trousseau des clés retirées, sans doublon. La clé est validée avant écriture.
pub fn retire_key(config_dir: &Path, key_hex: &str) -> AppResult<()> {
    let key_hex = key_hex.trim();
    KeyEntry::from_hex(key_hex)?;
    let mut lines = read_lines(config_dir)?;
    if lines.iter().any(|line| line == key_hex) {
        return Ok(());
    }
    lines.push(key_hex.to_owned());
    fs::create_dir_all(config_dir)?;
    crate::worker_settings::write_private(&retired_keys_path(config_dir), &lines.join("\n"))
        .map_err(|error| {
            tracing::error!(error = %error, "écriture du trousseau de clés retirées impossible");
            AppError::Internal
        })
}
