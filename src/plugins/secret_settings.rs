//! Réglages de plugins : les champs secrets sont chiffrés au repos et ne quittent jamais le serveur en clair.

use super::settings::{SettingsField, SettingsFieldType};
use crate::{
    error::{AppError, AppResult},
    secrets::{SecretBox, is_encrypted},
};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Indices de nom qui désignent un secret même sans schéma (plugin inconnu, ancien enregistrement).
const SECRET_NAME_HINTS: [&str; 4] = ["key", "token", "secret", "password"];
/// Taille maximale des réglages enregistrés pour un plugin.
const MAX_SETTINGS_BYTES: usize = 64 * 1024;
const MAX_SETTINGS_KEYS: usize = 64;

/// Champs secrets : ceux que le schéma déclare, plus tout nom évocateur parmi les clés connues.
/// Données associées d'un secret de plugin : il ne se déchiffre que pour ce plugin et ce champ.
pub fn plugin_secret_aad(plugin_id: &str, field: &str) -> String {
    format!("plugin:{plugin_id}:{field}")
}

pub fn secret_names<'a>(
    fields: &[SettingsField],
    keys: impl Iterator<Item = &'a String>,
) -> HashSet<String> {
    let mut names: HashSet<String> = fields
        .iter()
        .filter(|field| field.field_type == SettingsFieldType::Secret)
        .map(|field| field.name.clone())
        .collect();
    names.extend(keys.filter(|key| looks_secret(key)).cloned());
    names
}

fn looks_secret(name: &str) -> bool {
    let lowered = name.to_lowercase();
    SECRET_NAME_HINTS.iter().any(|hint| lowered.contains(hint))
}

fn as_object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// Prépare l'enregistrement : chiffre les secrets saisis, conserve ceux déjà enregistrés si le champ est laissé vide.
pub fn seal(
    secrets: &SecretBox,
    plugin_id: &str,
    incoming: &Value,
    existing: &Value,
    secret_names: &HashSet<String>,
) -> AppResult<Value> {
    let incoming = incoming.as_object().ok_or(AppError::Invalid)?;
    if incoming.len() > MAX_SETTINGS_KEYS
        || serde_json::to_string(incoming)
            .map_err(|_| AppError::Invalid)?
            .len()
            > MAX_SETTINGS_BYTES
    {
        return Err(AppError::TooLarge);
    }
    let existing = as_object(existing);
    let mut sealed = Map::new();
    for (name, value) in incoming {
        if !secret_names.contains(name) {
            sealed.insert(name.clone(), value.clone());
            continue;
        }
        match value.as_str() {
            Some(text) if !text.is_empty() => {
                sealed.insert(
                    name.clone(),
                    Value::String(secrets.encrypt(text, &plugin_secret_aad(plugin_id, name))?),
                );
            }
            _ => {
                if let Some(previous) = existing.get(name) {
                    sealed.insert(name.clone(), previous.clone());
                }
            }
        }
    }
    for name in secret_names {
        if !sealed.contains_key(name)
            && let Some(previous) = existing.get(name)
        {
            sealed.insert(name.clone(), previous.clone());
        }
    }
    Ok(Value::Object(sealed))
}

/// Chiffre les secrets encore en clair d'un ancien enregistrement ; `None` si rien à migrer.
pub fn migrate_plaintext(
    secrets: &SecretBox,
    plugin_id: &str,
    stored: &Value,
    secret_names: &HashSet<String>,
) -> AppResult<Option<Value>> {
    let mut object = as_object(stored);
    let mut changed = false;
    for name in secret_names {
        if let Some(Value::String(text)) = object.get(name)
            && !text.is_empty()
            && !is_encrypted(text)
        {
            let encrypted = secrets.encrypt(text, &plugin_secret_aad(plugin_id, name))?;
            object.insert(name.clone(), Value::String(encrypted));
            changed = true;
        }
    }
    Ok(changed.then_some(Value::Object(object)))
}

/// Re-chiffre avec la nouvelle clé les secrets d'un enregistrement de réglages ; `None` s'il n'y en a pas.
/// Renvoie les réglages réécrits et le nombre de valeurs changées.
pub fn rotate_settings(
    old: &SecretBox,
    new: &SecretBox,
    plugin_id: &str,
    stored: &str,
) -> AppResult<Option<(String, usize)>> {
    let Ok(mut value) = serde_json::from_str::<Value>(stored) else {
        return Ok(None);
    };
    let Some(object) = value.as_object_mut() else {
        return Ok(None);
    };
    let mut rotated = 0;
    for (name, field) in object.iter_mut() {
        let Some(text) = field.as_str().filter(|text| is_encrypted(text)) else {
            continue;
        };
        let aad = plugin_secret_aad(plugin_id, name);
        let plaintext = old.decrypt(text, &aad)?;
        *field = Value::String(new.encrypt(&plaintext, &aad)?);
        rotated += 1;
    }
    if rotated == 0 {
        return Ok(None);
    }
    let rewritten = serde_json::to_string(&value).map_err(|_| AppError::Invalid)?;
    Ok(Some((rewritten, rotated)))
}

/// Réglages sans les secrets, et noms des secrets qui ont une valeur enregistrée.
pub fn mask(stored: &Value, secret_names: &HashSet<String>) -> (Value, HashSet<String>) {
    let mut public = as_object(stored);
    let mut set = HashSet::new();
    for name in secret_names {
        if let Some(value) = public.remove(name)
            && value.as_str().is_some_and(|text| !text.is_empty())
        {
            set.insert(name.clone());
        }
    }
    (Value::Object(public), set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PLUGIN: &str = "plugin.demo";

    fn secret_field(name: &str) -> SettingsField {
        SettingsField {
            name: name.to_owned(),
            label: name.to_owned(),
            field_type: SettingsFieldType::Secret,
            default_value: None,
            required: true,
            description: None,
        }
    }

    fn names(fields: &[SettingsField], keys: &[&str]) -> HashSet<String> {
        let keys: Vec<String> = keys.iter().map(|key| (*key).to_owned()).collect();
        secret_names(fields, keys.iter())
    }

    #[test]
    fn test_secret_names_combine_schema_and_name_hints() {
        let found = names(
            &[secret_field("custom")],
            &["url", "apiKey", "authToken", "pathMapping"],
        );
        assert!(found.contains("custom"));
        assert!(found.contains("apiKey"));
        assert!(found.contains("authToken"));
        assert!(!found.contains("url"));
        assert!(!found.contains("pathMapping"));
    }

    #[test]
    fn test_seal_encrypts_new_secrets_and_keeps_existing_ones_when_left_empty() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let secret = names(&[], &["apiKey"]);

        let first = seal(
            &secrets,
            PLUGIN,
            &json!({"url": "http://s", "apiKey": "clef"}),
            &json!({}),
            &secret,
        )?;
        let stored = first["apiKey"].as_str().unwrap_or_default().to_owned();
        assert!(is_encrypted(&stored));
        assert_eq!(first["url"], "http://s");
        assert_eq!(
            secrets.decrypt(&stored, &plugin_secret_aad(PLUGIN, "apiKey"))?,
            "clef"
        );

        let kept = seal(
            &secrets,
            PLUGIN,
            &json!({"url": "http://t", "apiKey": ""}),
            &first,
            &secret,
        )?;
        assert_eq!(kept["apiKey"], stored);
        assert_eq!(kept["url"], "http://t");

        let omitted = seal(
            &secrets,
            PLUGIN,
            &json!({"url": "http://u"}),
            &first,
            &secret,
        )?;
        assert_eq!(omitted["apiKey"], stored);

        let replaced = seal(
            &secrets,
            PLUGIN,
            &json!({"apiKey": "nouvelle"}),
            &first,
            &secret,
        )?;
        assert_eq!(
            secrets.decrypt(
                replaced["apiKey"].as_str().unwrap_or_default(),
                &plugin_secret_aad(PLUGIN, "apiKey"),
            )?,
            "nouvelle"
        );
        Ok(())
    }

    #[test]
    fn test_seal_refuses_non_objects_and_oversized_payloads() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let none = HashSet::new();
        assert!(matches!(
            seal(&secrets, PLUGIN, &json!([1]), &json!({}), &none),
            Err(AppError::Invalid)
        ));
        let big = json!({"note": "x".repeat(MAX_SETTINGS_BYTES + 1)});
        assert!(matches!(
            seal(&secrets, PLUGIN, &big, &json!({}), &none),
            Err(AppError::TooLarge)
        ));
        let many: Map<String, Value> = (0..=MAX_SETTINGS_KEYS)
            .map(|index| (format!("k{index}"), Value::Bool(true)))
            .collect();
        assert!(matches!(
            seal(&secrets, PLUGIN, &Value::Object(many), &json!({}), &none),
            Err(AppError::TooLarge)
        ));
        Ok(())
    }

    #[test]
    fn test_migrate_plaintext_encrypts_only_clear_secrets() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let secret = names(&[], &["apiKey"]);
        let migrated = migrate_plaintext(
            &secrets,
            PLUGIN,
            &json!({"apiKey": "clair", "url": "http://s"}),
            &secret,
        )?
        .ok_or(AppError::Internal)?;
        assert!(is_encrypted(
            migrated["apiKey"].as_str().unwrap_or_default()
        ));
        assert_eq!(migrated["url"], "http://s");
        assert!(migrate_plaintext(&secrets, PLUGIN, &migrated, &secret)?.is_none());
        Ok(())
    }

    #[test]
    fn test_mask_removes_secrets_and_reports_which_are_set() {
        let secret = names(&[], &["apiKey", "token"]);
        let (public, set) = mask(
            &json!({"url": "http://s", "apiKey": "enc:v2:aa", "token": ""}),
            &secret,
        );
        assert_eq!(public, json!({"url": "http://s"}));
        assert!(set.contains("apiKey"));
        assert!(!set.contains("token"));
    }
}
