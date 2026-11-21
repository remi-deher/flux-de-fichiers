//! Gestion du dépôt local utilisateur (`/config/plugins/local`) sans dépendre d'un dépôt GitHub.

mod index;
mod snapshot;
mod write;

#[cfg(test)]
mod tests;

pub use index::rebuild_local_index;
pub use snapshot::{build_local_snapshot, local_resource_file};
pub(super) use snapshot::read_local_script;
pub use write::{
    delete_local_item, save_local_block_template, save_local_flow_template, save_local_pack,
    save_local_script, upload_local_resource,
};

use super::is_valid_repository_id;
use crate::error::{AppError, AppResult};
use serde_json::Value;
use std::{fs, path::Path};

pub(super) const MAX_SCRIPT_BYTES: usize = 512 * 1024;
pub(super) const MAX_RESOURCE_BYTES: usize = 5 * 1024 * 1024;
const MAX_SLUG_LEN: usize = 64;
const MAX_FILENAME_LEN: usize = 120;
const SLUGIFY_MAX_LEN: usize = 48;
pub(super) const BLOCK_TEMPLATE_TAG: &str = "block-template";

/// Normalise un libellé en slug ASCII sûr (`a-z0-9-`).
pub fn slugify(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().chars().flat_map(char::to_lowercase) {
        let ch = match ch {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        };
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "item".to_owned()
    } else {
        trimmed.chars().take(SLUGIFY_MAX_LEN).collect()
    }
}

pub(super) fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FILENAME_LEN
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

pub(super) fn normalize_description(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(super) fn write_json_pretty(path: &Path, value: &Value) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let formatted = serde_json::to_string_pretty(value)
        .map_err(|err| AppError::InvalidManifest(err.to_string()))?;
    fs::write(path, formatted)?;
    Ok(())
}

/// Résout `(slug, is_update)` en validant strictement le préfixe lors d'une mise à jour.
pub(super) fn resolve_slug(
    id: Option<&str>,
    prefix: &str,
    name: &str,
) -> AppResult<(String, bool)> {
    match id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(existing) => {
            let Some(rest) = existing.strip_prefix(prefix) else {
                return Err(AppError::InvalidManifest(
                    "identifiant local invalide".into(),
                ));
            };
            if rest.is_empty()
                || rest.len() > MAX_SLUG_LEN
                || !rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                || !is_valid_repository_id(existing)
            {
                return Err(AppError::InvalidManifest(
                    "identifiant local invalide".into(),
                ));
            }
            Ok((rest.to_owned(), true))
        }
        None => {
            let slug = slugify(name);
            if !is_valid_repository_id(&format!("{prefix}{slug}")) {
                return Err(AppError::InvalidManifest(
                    "identifiant local invalide".into(),
                ));
            }
            Ok((slug, false))
        }
    }
}

pub(super) fn ensure_new_target_absent(base_dir: &Path, is_update: bool) -> AppResult<()> {
    if !is_update && base_dir.join("manifest.json").exists() {
        return Err(AppError::Conflict);
    }
    Ok(())
}
