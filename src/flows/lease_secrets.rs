//! Secrets de plugins déchiffrés qui voyagent dans un `Claim` ou un worker local :
//! uniquement ceux référencés par `runtime.settingsPluginId` des opérations du flux figé.

use crate::{
    flows::contract::OperationRegistry,
    plugins::plugin_secret_aad,
    repository,
    secrets::{SecretBox, is_encrypted},
};
use flux_shared::flows::FlowRecord;
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::{collections::HashMap, path::Path};

/// Charge et déchiffre les réglages des seuls plugins utilisés par `flow` (vide si aucun flux).
pub async fn load_flow_plugin_configs(
    pool: &SqlitePool,
    secrets_dir: &Path,
    registry: &OperationRegistry,
    flow: Option<&FlowRecord>,
) -> HashMap<String, Value> {
    let plugin_ids = registry.required_settings_plugins_for_flow(flow);
    if plugin_ids.is_empty() {
        return HashMap::new();
    }
    let Ok(secrets) = SecretBox::open(secrets_dir) else {
        return HashMap::new();
    };
    let mut configs = HashMap::new();
    for plugin_id in plugin_ids {
        let Ok(Some(raw)) = repository::get_plugin_settings(pool, &plugin_id).await else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if let Some(decrypted) = decrypt_plugin_settings_value(&secrets, &plugin_id, value) {
            configs.insert(plugin_id, decrypted);
        }
    }
    configs
}

/// Déchiffre les champs chiffrés d'un objet de réglages de plugin pour exécution par un worker.
pub fn decrypt_plugin_settings_value(
    secrets: &SecretBox,
    plugin_id: &str,
    value: Value,
) -> Option<Value> {
    let obj = value.as_object()?;
    let mut out = Map::with_capacity(obj.len());
    for (field, val) in obj {
        match val.as_str() {
            Some(text) if is_encrypted(text) => {
                let clear = secrets
                    .decrypt(text, &plugin_secret_aad(plugin_id, field))
                    .ok()?;
                out.insert(field.clone(), Value::String(clear));
            }
            _ => {
                out.insert(field.clone(), val.clone());
            }
        }
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::contract::OperationRegistry;
    use serde_json::json;

    fn sample_registry() -> OperationRegistry {
        let mut reg = OperationRegistry::new();
        let _ = reg.register_json(json!({
            "schemaVersion": "1.1.0",
            "id": "operation.sonarr.rescan",
            "version": "1.0.0",
            "name": "Sonarr Rescan",
            "providerId": "plugin.sonarr",
            "providerKind": "plugin",
            "capability": "integration",
            "status": "validated",
            "runtime": {
                "kind": "http",
                "settingsPluginId": "plugin.sonarr",
                "requiredSettings": ["url", "apiKey"],
                "steps": [{ "url": "{$setting.url}/api", "method": "POST" }]
            }
        }));
        reg.register_block_mapping("block.sonarr.rescan", "operation.sonarr.rescan");
        reg
    }

    #[test]
    fn decrypts_encrypted_plugin_settings_and_keeps_plaintext() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let sealed =
            secrets.encrypt("api_secret", &plugin_secret_aad("plugin.sonarr", "apiKey"))?;
        let raw = json!({ "url": "http://sonarr:8989", "apiKey": sealed });
        let decrypted = decrypt_plugin_settings_value(&secrets, "plugin.sonarr", raw)
            .ok_or_else(|| anyhow::anyhow!("decryption failed"))?;
        assert_eq!(decrypted["url"], "http://sonarr:8989");
        assert_eq!(decrypted["apiKey"], "api_secret");

        let reg = sample_registry();
        let flow = FlowRecord {
            id: "f1".into(),
            version: 1,
            name: "test".into(),
            description: None,
            file_mask: "*".into(),
            is_default: false,
            definition: json!({
                "nodes": [{ "id": "n1", "type": "block.sonarr.rescan", "name": "s" }],
                "connections": []
            })
            .to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert_eq!(
            reg.required_settings_plugins_for_flow(Some(&flow)),
            vec!["plugin.sonarr".to_owned()]
        );
        Ok(())
    }
}
