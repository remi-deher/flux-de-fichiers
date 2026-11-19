//! Tests d'intégration du durcissement backend : escrime de bail (`lease_epoch`),
//! chiffrement par bail des secrets sur `/api/v1/claims`, télémétrie/métadonnées worker
//! et réglages réseau/SMTP/sortie éditables.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{AppState, router},
    config::Config,
    domain::Claim,
    flows::lease_secrets::unseal_claim_secrets_for_worker,
    repository,
    telemetry::TelemetrySnapshot,
};
use forgearr_shared::system::{SmtpTls, UpdateSystemSettingsRequest};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;
use uuid::Uuid;

const JSON_LIMIT: usize = 64 * 1024;
const SOURCE_CONTENT: &[u8] = b"flux-mkv-demo-bytes";

struct Harness {
    app: axum::Router,
    pool: SqlitePool,
    _dirs: (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir),
}

fn write_plugin_catalog(root: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(root.join("plugins/core/blocks"))?;
    std::fs::create_dir_all(root.join("plugins/core/operations"))?;
    std::fs::write(
        root.join("index.json"),
        serde_json::json!({
            "schemaVersion": "1.0.0",
            "entries": [
                {
                    "id": "essentiel",
                    "type": "plugin",
                    "version": "1.0.0",
                    "name": "Noyau",
                    "manifestPath": "plugins/core/manifest.json"
                },
                {
                    "id": "essentiel.webhook",
                    "type": "block",
                    "owner": "essentiel",
                    "version": "1.0.0",
                    "name": "Webhook",
                    "manifestPath": "plugins/core/blocks/webhook.json"
                }
            ]
        })
        .to_string(),
    )?;
    std::fs::write(
        root.join("plugins/core/manifest.json"),
        serde_json::json!({
            "schemaVersion": "1.1.0",
            "id": "essentiel",
            "type": "plugin",
            "version": "1.0.0",
            "name": "Noyau",
            "files": [
                {"path": "blocks/webhook.json", "role": "config", "mediaType": "application/json"},
                {"path": "operations/webhook.json", "role": "operation", "mediaType": "application/json"}
            ],
            "settingsSchema": {
                "fields": [
                    {"name": "endpoint", "label": "URL", "type": "url", "default": "https://hooks.example"},
                    {"name": "token", "label": "Jeton", "type": "secret", "required": true}
                ]
            }
        })
        .to_string(),
    )?;
    std::fs::write(
        root.join("plugins/core/blocks/webhook.json"),
        serde_json::json!({
            "schemaVersion": "1.1.0",
            "id": "essentiel.webhook",
            "aliases": ["webhook"],
            "version": "1.0.0",
            "name": "Webhook",
            "description": "Appelle une URL",
            "category": "io",
            "operationId": "operation.core.webhook",
            "ui": {"category": "Notification"}
        })
        .to_string(),
    )?;
    std::fs::write(
        root.join("plugins/core/operations/webhook.json"),
        serde_json::json!({
            "schemaVersion": "1.1.0",
            "id": "operation.core.webhook",
            "version": "1.0.0",
            "name": "Webhook",
            "providerId": "essentiel",
            "providerKind": "plugin",
            "capability": "notification",
            "status": "validated",
            "runtime": {
                "kind": "http",
                "settingsPluginId": "essentiel",
                "requiredSettings": ["endpoint", "token"],
                "steps": [{
                    "url": "{$settings.endpoint}",
                    "method": "POST",
                    "mergeCustomHeaders": true
                }]
            }
        })
        .to_string(),
    )?;
    Ok(())
}

async fn harness() -> anyhow::Result<Harness> {
    let data_dir = tempfile::tempdir()?;
    let config_dir = tempfile::tempdir()?;
    let catalog_dir = tempfile::tempdir()?;
    let input_dir = data_dir.path().join("input");
    std::fs::create_dir_all(&input_dir)?;
    write_plugin_catalog(catalog_dir.path())?;

    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: data_dir.path().to_path_buf(),
        input_dir,
        config_dir: config_dir.path().to_path_buf(),
        tools_cache_dir: config_dir.path().join("tools-cache"),
        plugins_dir: Some(catalog_dir.path().to_path_buf()),
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024 * 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;

    let app = router(AppState::new(pool.clone(), Arc::new(config)));
    Ok(Harness {
        app,
        pool,
        _dirs: (data_dir, config_dir, catalog_dir),
    })
}

async fn json_call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: Option<serde_json::Value>,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(key) = bearer {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    let body = match body {
        Some(json) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&json)?)
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(builder.body(body)?).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), JSON_LIMIT).await?;
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    Ok((status, value))
}

#[tokio::test]
async fn lease_epoch_fences_all_worker_job_routes() -> anyhow::Result<()> {
    let h = harness().await?;

    // Crée une tâche en attente associée à une source factice.
    let job = repository::insert_job(
        &h.pool,
        "video.mkv",
        None,
        &"aa".repeat(32),
        SOURCE_CONTENT.len() as i64,
    )
    .await?;

    let (status, worker_json) = json_call(
        &h.app,
        "POST",
        "/api/v1/workers",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"name": "nas-fencing"})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let worker_id = Uuid::parse_str(worker_json["id"].as_str().unwrap_or_default())?;

    let (status, claim_json) = json_call(
        &h.app,
        "POST",
        "/api/v1/claims",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"worker_id": worker_id})),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let claim: Claim = serde_json::from_value(claim_json)?;
    assert_eq!(claim.lease_epoch, 1);

    let stale_epoch = claim.lease_epoch + 1;

    // Chaque route vérifiée par bail rejette un `lease_epoch` incohérent avec 409 Conflict.
    for (method, uri, body) in [
        (
            "POST",
            format!("/api/v1/jobs/{}/heartbeat", job.id),
            Some(serde_json::json!({
                "worker_id": worker_id,
                "lease_token": claim.lease_token,
                "lease_epoch": stale_epoch,
            })),
        ),
        (
            "POST",
            format!("/api/v1/jobs/{}/progress", job.id),
            Some(serde_json::json!({
                "worker_id": worker_id,
                "lease_token": claim.lease_token,
                "lease_epoch": stale_epoch,
                "progress": 25,
            })),
        ),
        (
            "POST",
            format!("/api/v1/jobs/{}/steps", job.id),
            Some(serde_json::json!({
                "worker_id": worker_id,
                "lease_token": claim.lease_token,
                "lease_epoch": stale_epoch,
                "steps": [{
                    "step_index": 0,
                    "node_id": "n1",
                    "block_kind": "essentiel.file-input",
                    "status": "succeeded"
                }],
            })),
        ),
        (
            "POST",
            format!("/api/v1/jobs/{}/failure", job.id),
            Some(serde_json::json!({
                "worker_id": worker_id,
                "lease_token": claim.lease_token,
                "lease_epoch": stale_epoch,
                "error_code": "source_integrity",
            })),
        ),
    ] {
        let (st, _) = json_call(&h.app, method, &uri, Some(common::WORKER_KEY), body).await?;
        assert_eq!(st, StatusCode::CONFLICT, "{uri}");
    }

    for (method, uri, payload) in [
        (
            "GET",
            format!("/api/v1/jobs/{}/source", job.id),
            Body::empty(),
        ),
        (
            "PUT",
            format!("/api/v1/jobs/{}/result", job.id),
            Body::from("out"),
        ),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(&uri)
            .header("authorization", format!("Bearer {}", common::WORKER_KEY))
            .header("x-worker-id", worker_id.to_string())
            .header("x-lease-token", claim.lease_token.to_string())
            .header("x-lease-epoch", stale_epoch.to_string())
            .body(payload)?;
        let resp = h.app.clone().oneshot(req).await?;
        assert_eq!(resp.status(), StatusCode::CONFLICT, "{uri}");
    }

    Ok(())
}

#[tokio::test]
async fn claim_seals_plugin_and_webhook_secrets_for_the_worker_only() -> anyhow::Result<()> {
    let h = harness().await?;

    // 1. Enregistre un secret de plugin et un flux contenant un webhook secret.
    let (st, _) = json_call(
        &h.app,
        "PUT",
        "/api/v1/plugins/essentiel/settings",
        Some(common::ADMIN_KEY),
        Some(serde_json::json!({
            "endpoint": "https://hooks.example/run",
            "token": "PLUGIN-TOP-SECRET"
        })),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);

    let definition = serde_json::json!({
        "format": 1,
        "nodes": [{
            "id": "hook-1",
            "type": "essentiel.webhook",
            "name": "Webhook",
            "config": {
                "url": "https://hooks.example/event",
                "custom_headers": {
                    "Authorization": "Bearer HOOK-TOP-SECRET"
                }
            }
        }],
        "connections": []
    })
    .to_string();

    let (st, _) = json_call(
        &h.app,
        "PUT",
        "/api/v1/flows/flow.secrets",
        Some(common::ADMIN_KEY),
        Some(serde_json::json!({
            "name": "Flux secrets",
            "description": null,
            "file_mask": "*.mkv",
            "is_default": true,
            "definition": definition
        })),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);

    repository::insert_job(&h.pool, "film.mkv", None, &"bb".repeat(32), 1024).await?;

    let (st, worker_json) = json_call(
        &h.app,
        "POST",
        "/api/v1/workers",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"name": "nas-sealing"})),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    let worker_id = Uuid::parse_str(worker_json["id"].as_str().unwrap_or_default())?;

    let (st, claim_json) = json_call(
        &h.app,
        "POST",
        "/api/v1/claims",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"worker_id": worker_id})),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);

    let wire_text = claim_json.to_string();
    assert!(!wire_text.contains("PLUGIN-TOP-SECRET"));
    assert!(!wire_text.contains("HOOK-TOP-SECRET"));
    assert!(
        claim_json["sealed_secrets"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );

    let mut claim: Claim = serde_json::from_value(claim_json)?;
    assert!(claim.plugin_configs.is_empty());

    // Une autre clé worker ne peut pas désceller le bail.
    let mut wrong = claim.clone();
    assert!(
        unseal_claim_secrets_for_worker(
            &mut wrong,
            "fdf_b9999999_000000000000000000000000000000000000000000000000",
        )
        .is_err()
    );

    // La clé du worker détenteur restaure réglages plugins et en-têtes du flux figé.
    unseal_claim_secrets_for_worker(&mut claim, common::WORKER_KEY)?;
    assert!(claim.sealed_secrets.is_none());
    assert_eq!(
        claim.plugin_configs["essentiel"]["token"],
        "PLUGIN-TOP-SECRET"
    );
    let def_clear = claim
        .frozen_flow
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("flux figé attendu"))?
        .definition
        .clone();
    assert!(def_clear.contains("Bearer HOOK-TOP-SECRET"));
    Ok(())
}

#[tokio::test]
async fn worker_version_remote_addr_and_telemetry_are_persisted() -> anyhow::Result<()> {
    let h = harness().await?;
    let telemetry = TelemetrySnapshot {
        timestamp: chrono::Utc::now(),
        logical_cores: 8,
        cpu_usage_percent: 24.5,
        load_average: [1.0, 0.5, 0.25],
        ram_total_bytes: 16 * 1024 * 1024 * 1024,
        ram_available_bytes: 12 * 1024 * 1024 * 1024,
        ram_used_bytes: 4 * 1024 * 1024 * 1024,
        gpu_detected: true,
        gpu_devices: vec!["nvidia RTX 4060".into()],
        capabilities: Default::default(),
        storage_total_bytes: 500 * 1024 * 1024 * 1024,
        storage_available_bytes: 300 * 1024 * 1024 * 1024,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/workers")
        .header("authorization", format!("Bearer {}", common::WORKER_KEY))
        .header("content-type", "application/json")
        .header("x-forwarded-for", "198.51.100.42, 10.0.0.1")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "nas-telemetry",
            "version": "0.27.0-test",
            "telemetry": telemetry,
        }))?))?;
    let resp = h.app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let (st, list) = json_call(
        &h.app,
        "GET",
        "/api/v1/workers",
        Some(common::ADMIN_KEY),
        None,
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    let arr = list
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tableau attendu"))?;
    let worker = arr
        .iter()
        .find(|w| w["name"] == "nas-telemetry")
        .ok_or_else(|| anyhow::anyhow!("worker attendu"))?;
    assert_eq!(worker["version"], "0.27.0-test");
    assert_eq!(worker["remote_addr"], "198.51.100.42");
    assert_eq!(worker["telemetry"]["logicalCores"], 8);
    assert_eq!(worker["telemetry"]["gpuDevices"][0], "nvidia RTX 4060");

    let worker_id = Uuid::parse_str(worker["id"].as_str().unwrap_or_default())?;
    let job = repository::insert_job(&h.pool, "probe.mkv", None, &"cc".repeat(32), 512).await?;
    let (st, claim_json) = json_call(
        &h.app,
        "POST",
        "/api/v1/claims",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"worker_id": worker_id})),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    let claim: Claim = serde_json::from_value(claim_json)?;

    // Le heartbeat met à jour la télémétrie.
    let updated = TelemetrySnapshot {
        cpu_usage_percent: 71.0,
        ..telemetry
    };
    let (st, _) = json_call(
        &h.app,
        "POST",
        &format!("/api/v1/jobs/{}/heartbeat", job.id),
        Some(common::WORKER_KEY),
        Some(serde_json::json!({
            "worker_id": worker_id,
            "lease_token": claim.lease_token,
            "lease_epoch": claim.lease_epoch,
            "telemetry": updated,
        })),
    )
    .await?;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let (st, list) = json_call(
        &h.app,
        "GET",
        "/api/v1/workers",
        Some(common::ADMIN_KEY),
        None,
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    let worker = list
        .as_array()
        .and_then(|arr| arr.iter().find(|w| w["name"] == "nas-telemetry"))
        .ok_or_else(|| anyhow::anyhow!("worker attendu"))?;
    assert_eq!(worker["telemetry"]["cpuUsagePercent"], 71.0);
    Ok(())
}

#[tokio::test]
async fn system_settings_can_be_updated_and_smtp_password_stays_hidden() -> anyhow::Result<()> {
    let h = harness().await?;

    let req = UpdateSystemSettingsRequest {
        web_origin: Some("https://flux.example".into()),
        session_ttl_seconds: 7200,
        smtp_enabled: true,
        smtp_host: Some("smtp.example".into()),
        smtp_port: Some(587),
        smtp_tls: SmtpTls::StartTls,
        smtp_username: Some("mailer".into()),
        smtp_password: Some("smtp-secret-password".into()),
        smtp_from: Some("Flux <noreply@flux.example>".into()),
        allow_private_plugin_hosts: true,
        block_private_integration_hosts: true,
        check_updates_enabled: false,
        timezone: None,
    };

    // Un worker ne peut pas modifier les réglages système.
    let (st, _) = json_call(
        &h.app,
        "PUT",
        "/api/v1/system/settings",
        Some(common::WORKER_KEY),
        Some(serde_json::to_value(&req)?),
    )
    .await?;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, updated) = json_call(
        &h.app,
        "PUT",
        "/api/v1/system/settings",
        Some(common::ADMIN_KEY),
        Some(serde_json::to_value(&req)?),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    assert!(!updated.to_string().contains("smtp-secret-password"));
    assert_eq!(updated["network"]["webOrigin"], "https://flux.example");
    assert_eq!(updated["network"]["sessionTtlSeconds"], 7200);
    assert_eq!(updated["smtp"]["configured"], true);
    assert_eq!(updated["smtp"]["host"], "smtp.example");
    assert_eq!(updated["smtp"]["passwordSet"], true);
    assert_eq!(updated["outbound"]["allowPrivatePluginHosts"], true);
    assert_eq!(updated["outbound"]["blockPrivateIntegrationHosts"], true);

    let (st, fetched) = json_call(
        &h.app,
        "GET",
        "/api/v1/system/status",
        Some(common::ADMIN_KEY),
        None,
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(fetched["network"]["webOrigin"], "https://flux.example");
    assert_eq!(fetched["smtp"]["passwordSet"], true);
    assert!(!fetched.to_string().contains("smtp-secret-password"));

    let (st, setup) = json_call(&h.app, "GET", "/api/v1/setup/status", None, None).await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(setup["passwordResetEnabled"], true);

    // Valeur invalide (TTL hors bornes) -> 400 Bad Request.
    let invalid = UpdateSystemSettingsRequest {
        session_ttl_seconds: 5,
        ..req
    };
    let (st, _) = json_call(
        &h.app,
        "PUT",
        "/api/v1/system/settings",
        Some(common::ADMIN_KEY),
        Some(serde_json::to_value(&invalid)?),
    )
    .await?;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    Ok(())
}

/// Enregistre un worker de test avec ses encodeurs matériels et renvoie son identifiant.
async fn register_worker_with_encoders(
    h: &Harness,
    name: &str,
    codecs: &[forgearr_shared::capabilities::VideoCodec],
) -> anyhow::Result<Uuid> {
    use forgearr_shared::capabilities::{EncodeAcceleration, HardwareEncoder};
    let mut telemetry = TelemetrySnapshot {
        timestamp: chrono::Utc::now(),
        logical_cores: 4,
        cpu_usage_percent: 1.0,
        load_average: [0.0; 3],
        ram_total_bytes: 1,
        ram_available_bytes: 1,
        ram_used_bytes: 0,
        gpu_detected: false,
        gpu_devices: Vec::new(),
        capabilities: Default::default(),
        storage_total_bytes: 1,
        storage_available_bytes: 1,
    };
    telemetry.capabilities.hardware_encoders = codecs
        .iter()
        .map(|codec| HardwareEncoder {
            codec: *codec,
            acceleration: EncodeAcceleration::Nvenc,
        })
        .collect();
    let (st, worker) = json_call(
        &h.app,
        "POST",
        "/api/v1/workers",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"name": name, "version": "test", "telemetry": telemetry})),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    Ok(Uuid::parse_str(worker["id"].as_str().unwrap_or_default())?)
}

async fn claim_for(h: &Harness, worker: Uuid) -> anyhow::Result<Option<Claim>> {
    let (st, json) = json_call(
        &h.app,
        "POST",
        "/api/v1/claims",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"worker_id": worker})),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    Ok(serde_json::from_value(json)?)
}

fn capable_target(codec: &str) -> serde_json::Value {
    serde_json::json!({"mode": "capable", "requirement": {"codec": codec}})
}

async fn relay_call(
    h: &Harness,
    claim: &Claim,
    worker: Uuid,
    target: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let (st, response) = json_call(
        &h.app,
        "POST",
        &format!("/api/v1/jobs/{}/relay", claim.job_id),
        Some(common::WORKER_KEY),
        Some(serde_json::json!({
            "worker_id": worker,
            "lease_token": claim.lease_token,
            "lease_epoch": claim.lease_epoch,
            "request": {"target": target, "resumeNodeId": "outB"}
        })),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    Ok(response)
}

#[tokio::test]
async fn a_capable_relay_is_offered_to_every_capable_worker() -> anyhow::Result<()> {
    use forgearr_shared::capabilities::VideoCodec;
    let h = harness().await?;
    let origin = register_worker_with_encoders(&h, "sans-gpu", &[]).await?;
    let first = register_worker_with_encoders(&h, "gpu-un", &[VideoCodec::Hevc]).await?;
    let second = register_worker_with_encoders(&h, "gpu-deux", &[VideoCodec::Hevc]).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"aa".repeat(32), 512).await?;

    let claim = claim_for(&h, origin)
        .await?
        .ok_or_else(|| anyhow::anyhow!("le worker d'origine doit obtenir la tâche"))?;
    assert!(claim.resume_node_id.is_none());
    let response = relay_call(&h, &claim, origin, capable_target("hevc")).await?;
    assert_eq!(response["relayed"], true);
    assert!(
        response["target_worker_id"].is_null(),
        "aucun worker n'est épinglé"
    );

    // En file, non ciblée : l'origine (incapable) ne la reprend pas, un worker capable oui.
    let queued = repository::get_job(&h.pool, job.id).await?;
    assert_eq!(queued.status, forgearr::domain::JobStatus::Queued);
    assert_eq!(queued.target_worker_id, None);
    assert_eq!(queued.attempts, 0, "la tentative du relais est rendue");
    assert!(claim_for(&h, origin).await?.is_none());
    let resumed = claim_for(&h, second)
        .await?
        .ok_or_else(|| anyhow::anyhow!("un worker capable doit obtenir la tâche"))?;
    assert_eq!(resumed.job_id, job.id);
    assert_eq!(resumed.resume_node_id.as_deref(), Some("outB"));
    assert!(claim_for(&h, first).await?.is_none(), "déjà prise");

    let (st, infos) = json_call(
        &h.app,
        "GET",
        &format!("/api/v1/jobs/relays?ids={}", job.id),
        Some(common::ADMIN_KEY),
        None,
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(infos[0]["job_id"], job.id.to_string());
    assert_eq!(infos[0]["from_worker_id"], origin.to_string());
    assert_eq!(infos[0]["requirement"]["codec"], "hevc");
    Ok(())
}

#[tokio::test]
async fn a_precise_relay_waits_in_the_designated_worker_queue() -> anyhow::Result<()> {
    let h = harness().await?;
    let origin = register_worker_with_encoders(&h, "origine", &[]).await?;
    let precise = register_worker_with_encoders(&h, "designe", &[]).await?;
    let other = register_worker_with_encoders(&h, "autre", &[]).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"dd".repeat(32), 512).await?;
    let claim = claim_for(&h, origin)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tâche attendue"))?;
    let response = relay_call(
        &h,
        &claim,
        origin,
        serde_json::json!({"mode": "worker", "worker_id": precise}),
    )
    .await?;
    assert_eq!(response["target_worker_id"], precise.to_string());
    assert_eq!(
        repository::get_job(&h.pool, job.id).await?.target_worker_id,
        Some(precise)
    );
    assert!(
        claim_for(&h, other).await?.is_none(),
        "file propre au worker désigné"
    );
    assert!(claim_for(&h, precise).await?.is_some());
    Ok(())
}

#[tokio::test]
async fn a_relay_without_capable_worker_fails_the_job_visibly() -> anyhow::Result<()> {
    let h = harness().await?;
    let origin = register_worker_with_encoders(&h, "seul", &[]).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"bb".repeat(32), 512).await?;
    let claim = claim_for(&h, origin)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tâche attendue"))?;
    let response = relay_call(&h, &claim, origin, capable_target("av1")).await?;
    assert_eq!(response["relayed"], false);
    let failed = repository::get_job(&h.pool, job.id).await?;
    assert_eq!(failed.status, forgearr::domain::JobStatus::Failed);
    assert_eq!(
        failed.error_code,
        Some(forgearr::domain::JobErrorCode::NoCapableWorker)
    );
    Ok(())
}
