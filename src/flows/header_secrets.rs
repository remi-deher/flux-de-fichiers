//! Secrets d'un flux : chiffrés dans la définition, jamais renvoyés en clair par l'API.
//!
//! Deux objets de `config` portent des secrets : `custom_headers` (en-têtes d'un webhook, par
//! exemple `Authorization`) et `secrets` (variables de type secret d'un script, ADR 0025). À
//! l'enregistrement, les valeurs en clair sont chiffrées ; une valeur laissée vide conserve celle
//! déjà enregistrée pour le même bloc ; une valeur `null` retire l'entrée. L'API renvoie les noms
//! avec une valeur vide ; le moteur déchiffre à l'exécution.

use crate::{
    error::{AppError, AppResult},
    repository::DEFINITION_STORES,
    secrets::{SecretBox, is_encrypted},
};
use forgearr_shared::script_variables::{HEADERS_CONFIG_KEY, SECRETS_CONFIG_KEY};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Objet de `config` dont toutes les valeurs sont des secrets.
struct SecretObject {
    key: &'static str,
    /// Préfixe du nom d'un secret dans ses données associées et dans l'enveloppe d'un bail.
    prefix: &'static str,
    /// Une valeur vide sans valeur enregistrée retire l'entrée (un secret absent n'existe pas).
    drop_empty: bool,
}

static SECRET_OBJECTS: [SecretObject; 2] = [
    SecretObject {
        key: HEADERS_CONFIG_KEY,
        prefix: "",
        drop_empty: false,
    },
    SecretObject {
        key: SECRETS_CONFIG_KEY,
        prefix: "secret:",
        drop_empty: true,
    },
];

/// Objets secrets d'un bloc, avec leur description.
fn secret_maps(node: &mut Value) -> Vec<(&'static SecretObject, &mut Map<String, Value>)> {
    let Some(config) = node.get_mut("config").and_then(Value::as_object_mut) else {
        return Vec::new();
    };
    config
        .iter_mut()
        .filter_map(|(key, value)| {
            let object = SECRET_OBJECTS
                .iter()
                .find(|object| object.key == key.as_str())?;
            Some((object, value.as_object_mut()?))
        })
        .collect()
}

/// Secrets d'un bloc : nom de l'emplacement (préfixe compris) et valeur.
fn slots_of(node: &mut Value) -> Vec<(String, &mut Value)> {
    secret_maps(node)
        .into_iter()
        .flat_map(|(object, map)| {
            map.iter_mut()
                .map(move |(name, value)| (format!("{}{name}", object.prefix), value))
        })
        .collect()
}

fn nodes_of(definition: &mut Value) -> Vec<&mut Value> {
    definition
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .map(|nodes| nodes.iter_mut().collect())
        .unwrap_or_default()
}

fn node_id(node: &Value) -> String {
    node.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Données associées d'un secret : il ne se déchiffre que pour ce flux, ce bloc et cet emplacement.
fn header_aad(flow_id: &str, node_id: &str, slot: &str) -> String {
    format!("flow:{flow_id}:{node_id}:{slot}")
}

/// La définition déclare-t-elle des secrets (en-têtes de webhook ou variables de script) ?
pub fn has_flow_secrets(definition: &str) -> bool {
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return false;
    };
    nodes_of(&mut value)
        .into_iter()
        .any(|node| !slots_of(node).is_empty())
}

/// Valeurs déjà enregistrées, par bloc puis par emplacement.
fn existing_secrets(existing: Option<&str>) -> HashMap<(String, String), String> {
    let mut known = HashMap::new();
    let Some(Ok(mut value)) = existing.map(serde_json::from_str::<Value>) else {
        return known;
    };
    for node in nodes_of(&mut value) {
        let id = node_id(node);
        for (slot, secret) in slots_of(node) {
            if let Some(text) = secret.as_str() {
                known.insert((id.clone(), slot), text.to_owned());
            }
        }
    }
    known
}

/// Chiffre les secrets en clair d'une définition ; un secret laissé vide reprend sa valeur
/// enregistrée, un secret `null` est retiré.
pub fn seal_definition(
    secrets: &SecretBox,
    flow_id: &str,
    incoming: &str,
    existing: Option<&str>,
) -> AppResult<String> {
    let Ok(mut definition) = serde_json::from_str::<Value>(incoming) else {
        return Ok(incoming.to_owned());
    };
    let known = existing_secrets(existing);
    for node in nodes_of(&mut definition) {
        let id = node_id(node);
        for (object, map) in secret_maps(node) {
            map.retain(|_, value| !value.is_null());
            let mut dropped = Vec::new();
            for (name, value) in map.iter_mut() {
                let slot = format!("{}{name}", object.prefix);
                match value.as_str() {
                    Some("") => match known.get(&(id.clone(), slot)) {
                        Some(previous) => *value = Value::String(previous.clone()),
                        None if object.drop_empty => dropped.push(name.clone()),
                        None => {}
                    },
                    Some(text) if !is_encrypted(text) => {
                        *value =
                            Value::String(secrets.encrypt(text, &header_aad(flow_id, &id, &slot))?);
                    }
                    _ => {}
                }
            }
            for name in dropped {
                map.remove(&name);
            }
        }
    }
    serde_json::to_string(&definition).map_err(|_| AppError::Invalid)
}

/// Définition destinée à l'API : les noms de secrets restent, leurs valeurs sont vidées.
pub fn mask_definition(definition: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return definition.to_owned();
    };
    for node in nodes_of(&mut value) {
        for (_, secret) in slots_of(node) {
            *secret = Value::String(String::new());
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| definition.to_owned())
}

/// Remplace les valeurs chiffrées par leur clair, juste avant l'appel HTTP.
pub fn decrypt_headers(
    secrets: &SecretBox,
    flow_id: &str,
    node_id: &str,
    headers: &mut HashMap<String, String>,
) -> AppResult<()> {
    for (name, value) in headers.iter_mut() {
        if is_encrypted(value) {
            *value = secrets.decrypt(value, &header_aad(flow_id, node_id, name))?;
        }
    }
    Ok(())
}

/// Déchiffre tous les secrets d'une définition pour qu'un worker distant
/// puisse exécuter le flux sans la clé du central.
pub fn decrypt_definition_headers(
    secrets: &SecretBox,
    flow_id: &str,
    definition: &str,
) -> AppResult<String> {
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return Ok(definition.to_owned());
    };
    for node in nodes_of(&mut value) {
        let id = node_id(node);
        for (slot, secret) in slots_of(node) {
            let Some(text) = secret.as_str().filter(|text| is_encrypted(text)) else {
                continue;
            };
            *secret = Value::String(secrets.decrypt(text, &header_aad(flow_id, &id, &slot))?);
        }
    }
    Ok(serde_json::to_string(&value).unwrap_or_else(|_| definition.to_owned()))
}

/// Extrait les secrets en clair d'une définition (par bloc puis par emplacement) et renvoie la
/// définition masquée associée.
pub fn extract_and_mask_definition_headers(
    definition: &str,
) -> (String, HashMap<String, HashMap<String, String>>) {
    let mut extracted: HashMap<String, HashMap<String, String>> = HashMap::new();
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return (definition.to_owned(), extracted);
    };
    for node in nodes_of(&mut value) {
        let id = node_id(node);
        let mut by_slot = HashMap::new();
        for (slot, secret) in slots_of(node) {
            if let Some(text) = secret.as_str()
                && !text.is_empty()
            {
                by_slot.insert(slot, text.to_owned());
            }
            *secret = Value::String(String::new());
        }
        if !by_slot.is_empty() {
            extracted.insert(id, by_slot);
        }
    }
    let masked = serde_json::to_string(&value).unwrap_or_else(|_| definition.to_owned());
    (masked, extracted)
}

/// Réinjecte des secrets déchiffrés dans une définition de flux avant exécution.
pub fn inject_definition_headers(
    definition: &str,
    secrets_by_node: &HashMap<String, HashMap<String, String>>,
) -> String {
    if secrets_by_node.is_empty() {
        return definition.to_owned();
    }
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return definition.to_owned();
    };
    for node in nodes_of(&mut value) {
        let Some(node_secrets) = secrets_by_node.get(&node_id(node)) else {
            continue;
        };
        for (slot, secret) in slots_of(node) {
            if let Some(clear) = node_secrets.get(&slot) {
                *secret = Value::String(clear.clone());
            }
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| definition.to_owned())
}

/// Re-chiffre avec la nouvelle clé les secrets d'une définition de flux ; `None` s'il n'y en a pas.
/// Renvoie la définition réécrite et le nombre de valeurs changées.
pub fn rotate_definition(
    old: &SecretBox,
    new: &SecretBox,
    flow_id: &str,
    definition: &str,
) -> AppResult<Option<(String, usize)>> {
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return Ok(None);
    };
    let mut rotated = 0;
    for node in nodes_of(&mut value) {
        let id = node_id(node);
        for (slot, secret) in slots_of(node) {
            let Some(text) = secret.as_str().filter(|text| is_encrypted(text)) else {
                continue;
            };
            let aad = header_aad(flow_id, &id, &slot);
            let plaintext = old.decrypt(text, &aad)?;
            *secret = Value::String(new.encrypt(&plaintext, &aad)?);
            rotated += 1;
        }
    }
    if rotated == 0 {
        return Ok(None);
    }
    let rewritten = serde_json::to_string(&value).map_err(|_| AppError::Invalid)?;
    Ok(Some((rewritten, rotated)))
}

/// Chiffre au démarrage les en-têtes encore en clair des flux et de leurs versions.
pub async fn migrate_flow_secrets(pool: &SqlitePool, secrets: &SecretBox) -> AppResult<usize> {
    let mut migrated = 0;
    for store in DEFINITION_STORES {
        let rows = store.fetch_definitions(pool).await?;
        for (id, rowid, definition) in rows {
            if !has_flow_secrets(&definition) {
                continue;
            }
            let sealed = seal_definition(secrets, &id, &definition, None)?;
            if sealed != definition {
                store.update_definition(pool, rowid, &sealed).await?;
                migrated += 1;
            }
        }
    }
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FLOW: &str = "flow.test";

    fn secrets() -> anyhow::Result<(tempfile::TempDir, SecretBox)> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        Ok((dir, secrets))
    }

    fn definition(header_value: &str) -> String {
        json!({
            "nodes": [
                {"id": "n1", "type": "file-input", "name": "Entrée"},
                {"id": "n2", "type": "webhook", "name": "Notifier", "config": {
                    "url": "https://hook.example/x",
                    "custom_headers": {"Authorization": header_value}
                }}
            ],
            "connections": []
        })
        .to_string()
    }

    fn header_of(definition: &str) -> String {
        serde_json::from_str::<Value>(definition)
            .ok()
            .and_then(|value| {
                value["nodes"][1]["config"]["custom_headers"]["Authorization"]
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    }

    #[test]
    fn test_seal_encrypts_clear_headers_and_leaves_other_fields() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let sealed = seal_definition(&secrets, FLOW, &definition("Bearer SECRET"), None)?;
        assert!(!sealed.contains("Bearer SECRET"));
        assert!(is_encrypted(&header_of(&sealed)));
        assert!(sealed.contains("https://hook.example/x"));
        assert_eq!(
            secrets.decrypt(
                &header_of(&sealed),
                &header_aad(FLOW, "n2", "Authorization")
            )?,
            "Bearer SECRET"
        );
        Ok(())
    }

    #[test]
    fn test_empty_header_keeps_the_stored_value_of_the_same_block() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let first = seal_definition(&secrets, FLOW, &definition("Bearer SECRET"), None)?;
        let masked = mask_definition(&first);
        assert_eq!(header_of(&masked), "");

        let resaved = seal_definition(&secrets, FLOW, &masked, Some(&first))?;
        assert_eq!(
            secrets.decrypt(
                &header_of(&resaved),
                &header_aad(FLOW, "n2", "Authorization")
            )?,
            "Bearer SECRET"
        );

        let replaced = seal_definition(&secrets, FLOW, &definition("Bearer AUTRE"), Some(&first))?;
        assert_eq!(
            secrets.decrypt(
                &header_of(&replaced),
                &header_aad(FLOW, "n2", "Authorization")
            )?,
            "Bearer AUTRE"
        );
        Ok(())
    }

    #[test]
    fn test_sealing_twice_does_not_encrypt_an_encrypted_value_again() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let once = seal_definition(&secrets, FLOW, &definition("x"), None)?;
        let twice = seal_definition(&secrets, FLOW, &once, Some(&once))?;
        assert_eq!(header_of(&once), header_of(&twice));
        Ok(())
    }

    #[test]
    fn test_mask_keeps_header_names_and_hides_values() {
        let masked = mask_definition(&definition("Bearer SECRET"));
        assert!(!masked.contains("Bearer SECRET"));
        assert!(masked.contains("Authorization"));
        assert_eq!(mask_definition("pas du json"), "pas du json");
    }

    #[test]
    fn test_non_json_definitions_and_other_blocks_are_untouched() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        assert_eq!(
            seal_definition(&secrets, FLOW, "pas du json", None)?,
            "pas du json"
        );
        assert!(!has_flow_secrets("pas du json"));
        assert!(!has_flow_secrets(
            &json!({"nodes": [{"id": "a", "type": "file-input"}]}).to_string()
        ));
        assert!(has_flow_secrets(&definition("x")));
        Ok(())
    }

    #[test]
    fn test_decrypt_headers_restores_clear_values_only_for_encrypted_ones() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let mut headers = HashMap::from([
            (
                "Authorization".to_owned(),
                secrets.encrypt("Bearer SECRET", &header_aad(FLOW, "n2", "Authorization"))?,
            ),
            ("X-Plain".to_owned(), "tel quel".to_owned()),
        ]);
        decrypt_headers(&secrets, FLOW, "n2", &mut headers)?;
        assert_eq!(headers["Authorization"], "Bearer SECRET");
        assert_eq!(headers["X-Plain"], "tel quel");
        Ok(())
    }

    #[test]
    fn test_decrypt_definition_headers_clears_webhook_secrets_for_workers() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let sealed = seal_definition(&secrets, FLOW, &definition("Bearer SECRET"), None)?;
        assert!(header_of(&sealed).starts_with("enc:"));
        let clear = decrypt_definition_headers(&secrets, FLOW, &sealed)?;
        assert_eq!(header_of(&clear), "Bearer SECRET");
        assert_eq!(
            decrypt_definition_headers(&secrets, FLOW, "pas du json")?,
            "pas du json"
        );
        Ok(())
    }

    fn script_definition(token: Value) -> String {
        json!({
            "nodes": [
                {"id": "s1", "type": "script", "name": "Script", "config": {
                    "crf": 20,
                    "secrets": {"token": token}
                }}
            ],
            "connections": []
        })
        .to_string()
    }

    fn token_of(definition: &str) -> Value {
        serde_json::from_str::<Value>(definition)
            .map(|value| value["nodes"][0]["config"]["secrets"]["token"].clone())
            .unwrap_or_default()
    }

    #[test]
    fn test_script_secrets_are_sealed_masked_and_decrypted_like_headers() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        assert!(has_flow_secrets(&script_definition(json!("t0k"))));
        let sealed = seal_definition(&secrets, FLOW, &script_definition(json!("t0k")), None)?;
        let stored = token_of(&sealed);
        assert!(stored.as_str().is_some_and(is_encrypted));
        assert!(!sealed.contains("t0k"));
        assert!(sealed.contains("\"crf\":20"));

        assert_eq!(token_of(&mask_definition(&sealed)), json!(""));
        let clear = decrypt_definition_headers(&secrets, FLOW, &sealed)?;
        assert_eq!(token_of(&clear), json!("t0k"));
        Ok(())
    }

    #[test]
    fn test_script_secret_is_bound_to_its_flow_node_and_name() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let sealed = seal_definition(&secrets, FLOW, &script_definition(json!("t0k")), None)?;
        assert!(decrypt_definition_headers(&secrets, "autre.flux", &sealed).is_err());
        Ok(())
    }

    #[test]
    fn test_empty_script_secret_keeps_the_stored_value_and_null_removes_it() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let stored = seal_definition(&secrets, FLOW, &script_definition(json!("t0k")), None)?;

        let kept = seal_definition(&secrets, FLOW, &script_definition(json!("")), Some(&stored))?;
        assert_eq!(token_of(&kept), token_of(&stored));

        let removed = seal_definition(
            &secrets,
            FLOW,
            &script_definition(json!(null)),
            Some(&stored),
        )?;
        assert!(token_of(&removed).is_null());
        assert!(!has_flow_secrets(&removed));

        let never_set = seal_definition(&secrets, FLOW, &script_definition(json!("")), None)?;
        assert!(token_of(&never_set).is_null());
        Ok(())
    }

    #[test]
    fn test_script_secrets_travel_in_the_lease_envelope_and_come_back() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let sealed = seal_definition(&secrets, FLOW, &script_definition(json!("t0k")), None)?;
        let clear = decrypt_definition_headers(&secrets, FLOW, &sealed)?;
        let (masked, extracted) = extract_and_mask_definition_headers(&clear);
        assert!(!masked.contains("t0k"));
        assert_eq!(extracted["s1"]["secret:token"], "t0k");
        assert_eq!(
            token_of(&inject_definition_headers(&masked, &extracted)),
            json!("t0k")
        );
        Ok(())
    }

    #[test]
    fn test_rotation_rewrites_script_secrets_with_the_new_key() -> anyhow::Result<()> {
        let (_old_dir, old) = secrets()?;
        let (_new_dir, new) = secrets()?;
        let sealed = seal_definition(&old, FLOW, &script_definition(json!("t0k")), None)?;
        let (rewritten, count) = rotate_definition(&old, &new, FLOW, &sealed)?
            .ok_or_else(|| anyhow::anyhow!("rien à re-chiffrer"))?;
        assert_eq!(count, 1);
        let clear = decrypt_definition_headers(&new, FLOW, &rewritten)?;
        assert_eq!(token_of(&clear), json!("t0k"));
        Ok(())
    }

    #[tokio::test]
    async fn test_startup_migration_encrypts_stored_flows_once() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        sqlx::query(
            "INSERT INTO flows (id, name, file_mask, is_default, definition) VALUES ('f1', 'Flux', '*.mkv', 0, ?1)",
        )
        .bind(definition("Bearer SECRET"))
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO flow_versions (flow_id, version, name, file_mask, definition) VALUES ('f1', 1, 'Flux', '*.mkv', ?1)",
        )
        .bind(definition("Bearer SECRET"))
        .execute(&pool)
        .await?;

        assert_eq!(migrate_flow_secrets(&pool, &secrets).await?, 2);
        let version: String = sqlx::query_scalar("SELECT definition FROM flow_versions")
            .fetch_one(&pool)
            .await?;
        assert!(!version.contains("Bearer SECRET"));
        let stored: String = sqlx::query_scalar("SELECT definition FROM flows WHERE id = 'f1'")
            .fetch_one(&pool)
            .await?;
        assert!(!stored.contains("Bearer SECRET"));
        assert_eq!(migrate_flow_secrets(&pool, &secrets).await?, 0);
        Ok(())
    }
}
