//! En-têtes des blocs webhook : chiffrés dans la définition d'un flux, jamais renvoyés par l'API.
//!
//! Un en-tête (par exemple `Authorization`) est un secret. À l'enregistrement, les valeurs en clair
//! sont chiffrées ; une valeur laissée vide conserve celle déjà enregistrée pour le même bloc.
//! L'API renvoie les noms d'en-têtes avec une valeur vide ; le moteur déchiffre à l'exécution.

use super::webhook::WebhookBlockConfig;
use crate::{
    error::{AppError, AppResult},
    secrets::{SecretBox, is_encrypted},
};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;

const WEBHOOK_NODE_TYPE: &str = "webhook";
const HEADERS_KEY: &str = "custom_headers";

/// En-têtes du bloc si c'est un webhook qui en déclare.
fn headers_of(node: &mut Value) -> Option<&mut Map<String, Value>> {
    if node.get("type").and_then(Value::as_str) != Some(WEBHOOK_NODE_TYPE) {
        return None;
    }
    node.get_mut("config")?
        .get_mut(HEADERS_KEY)?
        .as_object_mut()
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

/// Données associées d'un en-tête : il ne se déchiffre que pour ce flux, ce bloc et cet en-tête.
fn header_aad(flow_id: &str, node_id: &str, header: &str) -> String {
    format!("flow:{flow_id}:{node_id}:{header}")
}

/// La définition déclare-t-elle des en-têtes de webhook ?
pub fn has_webhook_headers(definition: &str) -> bool {
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return false;
    };
    nodes_of(&mut value)
        .into_iter()
        .any(|node| headers_of(node).is_some_and(|headers| !headers.is_empty()))
}

/// Valeurs déjà enregistrées, par bloc puis par nom d'en-tête.
fn existing_headers(existing: Option<&str>) -> HashMap<(String, String), String> {
    let mut known = HashMap::new();
    let Some(Ok(mut value)) = existing.map(serde_json::from_str::<Value>) else {
        return known;
    };
    for node in nodes_of(&mut value) {
        let id = node_id(node);
        if let Some(headers) = headers_of(node) {
            for (name, header_value) in headers.iter() {
                if let Some(text) = header_value.as_str() {
                    known.insert((id.clone(), name.clone()), text.to_owned());
                }
            }
        }
    }
    known
}

/// Chiffre les en-têtes en clair d'une définition ; un en-tête laissé vide reprend sa valeur enregistrée.
pub fn seal_definition(
    secrets: &SecretBox,
    flow_id: &str,
    incoming: &str,
    existing: Option<&str>,
) -> AppResult<String> {
    let Ok(mut definition) = serde_json::from_str::<Value>(incoming) else {
        return Ok(incoming.to_owned());
    };
    let known = existing_headers(existing);
    for node in nodes_of(&mut definition) {
        let id = node_id(node);
        let Some(headers) = headers_of(node) else {
            continue;
        };
        for (name, value) in headers.iter_mut() {
            match value.as_str() {
                Some("") => {
                    if let Some(previous) = known.get(&(id.clone(), name.clone())) {
                        *value = Value::String(previous.clone());
                    }
                }
                Some(text) if !is_encrypted(text) => {
                    *value = Value::String(secrets.encrypt(text, &header_aad(flow_id, &id, name))?);
                }
                _ => {}
            }
        }
    }
    serde_json::to_string(&definition).map_err(|_| AppError::Invalid)
}

/// Définition destinée à l'API : les noms d'en-têtes restent, leurs valeurs sont vidées.
pub fn mask_definition(definition: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(definition) else {
        return definition.to_owned();
    };
    for node in nodes_of(&mut value) {
        if let Some(headers) = headers_of(node) {
            for header_value in headers.values_mut() {
                *header_value = Value::String(String::new());
            }
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| definition.to_owned())
}

/// Remplace les valeurs chiffrées par leur clair, juste avant l'appel HTTP.
pub fn decrypt_headers(
    secrets: &SecretBox,
    flow_id: &str,
    node_id: &str,
    config: &mut WebhookBlockConfig,
) -> AppResult<()> {
    for (name, value) in config.custom_headers.iter_mut() {
        if is_encrypted(value) {
            *value = secrets.decrypt(value, &header_aad(flow_id, node_id, name))?;
        }
    }
    Ok(())
}

/// Chiffre au démarrage les en-têtes encore en clair des flux déjà enregistrés.
pub async fn migrate_flow_secrets(pool: &SqlitePool, secrets: &SecretBox) -> AppResult<usize> {
    let flows: Vec<(String, String)> = sqlx::query_as("SELECT id, definition FROM flows")
        .fetch_all(pool)
        .await?;
    let mut migrated = 0;
    for (id, definition) in flows {
        if !has_webhook_headers(&definition) {
            continue;
        }
        let sealed = seal_definition(secrets, &id, &definition, None)?;
        if sealed != definition {
            sqlx::query("UPDATE flows SET definition = ?1 WHERE id = ?2")
                .bind(&sealed)
                .bind(&id)
                .execute(pool)
                .await?;
            migrated += 1;
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
        assert!(!has_webhook_headers("pas du json"));
        assert!(!has_webhook_headers(
            &json!({"nodes": [{"id": "a", "type": "file-input"}]}).to_string()
        ));
        assert!(has_webhook_headers(&definition("x")));
        Ok(())
    }

    #[test]
    fn test_decrypt_headers_restores_clear_values_only_for_encrypted_ones() -> anyhow::Result<()> {
        let (_dir, secrets) = secrets()?;
        let mut config = WebhookBlockConfig {
            url: "https://hook.example".into(),
            method: "POST".into(),
            custom_headers: HashMap::from([
                (
                    "Authorization".to_owned(),
                    secrets.encrypt("Bearer SECRET", &header_aad(FLOW, "n2", "Authorization"))?,
                ),
                ("X-Plain".to_owned(), "tel quel".to_owned()),
            ]),
            fail_on_error: false,
        };
        decrypt_headers(&secrets, FLOW, "n2", &mut config)?;
        assert_eq!(config.custom_headers["Authorization"], "Bearer SECRET");
        assert_eq!(config.custom_headers["X-Plain"], "tel quel");
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

        assert_eq!(migrate_flow_secrets(&pool, &secrets).await?, 1);
        let stored: String = sqlx::query_scalar("SELECT definition FROM flows WHERE id = 'f1'")
            .fetch_one(&pool)
            .await?;
        assert!(!stored.contains("Bearer SECRET"));
        assert_eq!(migrate_flow_secrets(&pool, &secrets).await?, 0);
        Ok(())
    }
}
