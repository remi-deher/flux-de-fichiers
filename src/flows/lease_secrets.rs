//! Secrets de plugins déchiffrés qui voyagent dans un `Claim` ou un worker local :
//! uniquement ceux référencés par `runtime.settingsPluginId` des opérations du flux figé.

use crate::{
    domain::Claim,
    error::{AppError, AppResult},
    flows::contract::OperationRegistry,
    flows::{extract_and_mask_definition_headers, inject_definition_headers},
    plugins::plugin_secret_aad,
    repository,
    secrets::{SecretBox, is_encrypted},
};
use forgearr_shared::flows::FlowRecord;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::{collections::HashMap, path::Path};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct LeaseSecretsEnvelope {
    #[serde(default)]
    plugin_configs: HashMap<String, Value>,
    #[serde(default)]
    flow_headers: HashMap<String, HashMap<String, String>>,
}

fn lease_aad(job_id: Uuid, lease_epoch: i64) -> String {
    format!("lease:{job_id}:{lease_epoch}")
}

/// Chiffre `plugin_configs` et les en-têtes webhook de `frozen_flow` avec la clé du worker
/// détenant ce bail `(job_id, lease_epoch)`, et purge les valeurs en clair du `Claim` JSON.
pub fn seal_claim_secrets_for_worker(claim: &mut Claim, worker_bearer_key: &str) -> AppResult<()> {
    let plugin_configs = std::mem::take(&mut claim.plugin_configs);
    let flow_headers = if let Some(ref mut flow) = claim.frozen_flow {
        let (masked, headers) = extract_and_mask_definition_headers(&flow.definition);
        flow.definition = masked;
        headers
    } else {
        HashMap::new()
    };
    if plugin_configs.is_empty() && flow_headers.is_empty() {
        claim.sealed_secrets = None;
        return Ok(());
    }
    let payload = LeaseSecretsEnvelope {
        plugin_configs,
        flow_headers,
    };
    let json = serde_json::to_string(&payload).map_err(|_| AppError::Internal)?;
    let box_for_lease =
        SecretBox::for_worker_lease(worker_bearer_key, claim.job_id, claim.lease_epoch)?;
    claim.sealed_secrets =
        Some(box_for_lease.encrypt(&json, &lease_aad(claim.job_id, claim.lease_epoch))?);
    Ok(())
}

/// Déchiffre l'enveloppe de secrets du bail sur le worker détenteur avant exécution du flux.
pub fn unseal_claim_secrets_for_worker(
    claim: &mut Claim,
    worker_bearer_key: &str,
) -> AppResult<()> {
    let Some(sealed) = claim.sealed_secrets.take() else {
        return Ok(());
    };
    let box_for_lease =
        SecretBox::for_worker_lease(worker_bearer_key, claim.job_id, claim.lease_epoch)?;
    let raw = box_for_lease.decrypt(&sealed, &lease_aad(claim.job_id, claim.lease_epoch))?;
    let payload: LeaseSecretsEnvelope =
        serde_json::from_str(&raw).map_err(|_| AppError::Invalid)?;
    claim.plugin_configs = payload.plugin_configs;
    if let Some(ref mut flow) = claim.frozen_flow {
        flow.definition = inject_definition_headers(&flow.definition, &payload.flow_headers);
    }
    Ok(())
}

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
            "providerId": "sonarr",
            "providerKind": "plugin",
            "capability": "integration",
            "status": "validated",
            "runtime": {
                "kind": "http",
                "settingsPluginId": "sonarr",
                "requiredSettings": ["url", "apiKey"],
                "steps": [{ "url": "{$setting.url}/api", "method": "POST" }]
            }
        }));
        reg.register_block_mapping("sonarr.rescan", "operation.sonarr.rescan");
        reg
    }

    #[test]
    fn decrypts_encrypted_plugin_settings_and_keeps_plaintext() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let sealed = secrets.encrypt("api_secret", &plugin_secret_aad("sonarr", "apiKey"))?;
        let raw = json!({ "url": "http://sonarr:8989", "apiKey": sealed });
        let decrypted = decrypt_plugin_settings_value(&secrets, "sonarr", raw)
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
                "nodes": [{ "id": "n1", "type": "sonarr.rescan", "name": "s" }],
                "connections": []
            })
            .to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert_eq!(
            reg.required_settings_plugins_for_flow(Some(&flow)),
            vec!["sonarr".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn seal_and_unseal_protect_claim_secrets_for_the_holder_only() -> anyhow::Result<()> {
        let mut claim = Claim {
            job_id: Uuid::from_u128(10),
            lease_token: Uuid::from_u128(20),
            lease_epoch: 3,
            source_name: "film.mkv".into(),
            source_sha256: "ab".repeat(32),
            source_bytes: 42,
            probe_tool_id: None,
            probe_version: None,
            required_tools: Vec::new(),
            library_id: None,
            frozen_flow: Some(FlowRecord {
                id: "flow.hook".into(),
                name: "Hook".into(),
                description: None,
                file_mask: "*.mkv".into(),
                is_default: false,
                definition: json!({
                    "nodes": [{
                        "id": "n1",
                        "type": "essentiel.webhook",
                        "name": "Webhook",
                        "config": {
                            "url": "https://hook.example/notify",
                            "custom_headers": { "Authorization": "Bearer secret-token" }
                        }
                    }],
                    "connections": []
                })
                .to_string(),
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }),
            operations: HashMap::new(),
            block_operations: HashMap::new(),
            plugin_configs: HashMap::from([(
                "sonarr".to_owned(),
                json!({ "url": "http://sonarr:8989", "apiKey": "sonarr-secret" }),
            )]),
            sealed_secrets: None,
            allow_private_integrations: None,
            media_root: None,
            local_source_root: None,
            resume_node_id: None,
        };

        let worker_key = "fdf_01020304_0123456789abcdef0123456789abcdef0123456789abcdef";
        seal_claim_secrets_for_worker(&mut claim, worker_key)?;
        assert!(claim.plugin_configs.is_empty());
        assert!(claim.sealed_secrets.is_some());
        let wire_def = &claim
            .frozen_flow
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("flow attendu"))?
            .definition;
        assert!(!wire_def.contains("Bearer secret-token"));
        assert!(!serde_json::to_string(&claim)?.contains("sonarr-secret"));

        // Mauvais worker ou mauvais epoch -> refusé.
        let mut wrong_epoch = claim.clone();
        wrong_epoch.lease_epoch = 4;
        assert!(unseal_claim_secrets_for_worker(&mut wrong_epoch, worker_key).is_err());

        let mut wrong_key = claim.clone();
        assert!(unseal_claim_secrets_for_worker(&mut wrong_key, "fdf_other_key").is_err());

        // Worker détenteur + bon epoch -> secrets restaurés.
        unseal_claim_secrets_for_worker(&mut claim, worker_key)?;
        assert!(claim.sealed_secrets.is_none());
        assert_eq!(claim.plugin_configs["sonarr"]["apiKey"], "sonarr-secret");
        let clear_def = &claim
            .frozen_flow
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("flow attendu"))?
            .definition;
        assert!(clear_def.contains("Bearer secret-token"));
        Ok(())
    }
}
