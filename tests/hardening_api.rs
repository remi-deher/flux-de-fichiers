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
    publication: forgearr::source_publication_broker::SourcePublicationBroker,
    inspection: forgearr::source_publication_broker::SourcePublicationBroker,
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

    let state = AppState::new(pool.clone(), Arc::new(config));
    let publication = state.source_publication.clone();
    let inspection = state.source_inspection.clone();
    let app = router(state);
    Ok(Harness {
        app,
        pool,
        publication,
        inspection,
        _dirs: (data_dir, config_dir, catalog_dir),
    })
}

#[tokio::test]
async fn read_only_inspections_use_a_separate_authenticated_channel() -> anyhow::Result<()> {
    use forgearr::domain::LeaseRequest;
    use forgearr_shared::{
        libraries::LibraryOutputPolicy,
        transfer::{SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask},
    };
    let h = harness().await?;
    let holder = repository::register(&h.pool, "test-worker").await?;
    let other = repository::register(&h.pool, "other-worker").await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = repository::claim(&h.pool, holder.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    let lease = LeaseRequest {
        worker_id: holder.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let request_id = Uuid::new_v4();
    let task = SourcePublicationTask {
        request_id,
        worker_id: holder.id,
        root: "/media".into(),
        source_name: "film.mkv".into(),
        source_sha256: "aa".repeat(32),
        source_bytes: 10,
        result_sha256: "bb".repeat(32),
        result_bytes: 9,
        extension: "mkv".into(),
        policy: LibraryOutputPolicy::ReplaceWithBackup,
        result_ticket: None,
    };
    let broker = h.inspection.clone();
    let pending_job = job.id;
    let waiting = tokio::spawn(async move {
        broker
            .request(
                task,
                "/unused".into(),
                pending_job,
                lease,
                std::time::Duration::from_secs(5),
            )
            .await
    });
    let ordinary = format!(
        "/api/v1/worker/source-publications/pending?worker_id={}",
        holder.id
    );
    assert!(
        json_call(&h.app, "GET", &ordinary, Some(common::WORKER_KEY), None)
            .await?
            .1
            .as_array()
            .is_some_and(|items| items.is_empty())
    );
    let wrong = format!(
        "/api/v1/worker/source-publication-inspections/pending?worker_id={}",
        other.id
    );
    assert_eq!(
        json_call(&h.app, "GET", &wrong, Some(common::WORKER_KEY), None)
            .await?
            .0,
        StatusCode::FORBIDDEN
    );
    let uri = format!(
        "/api/v1/worker/source-publication-inspections/pending?worker_id={}&wait_ms=3000",
        holder.id
    );
    let (status, tasks) = json_call(&h.app, "GET", &uri, Some(common::WORKER_KEY), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks[0]["request_id"], request_id.to_string());
    let report = SourcePublicationReport {
        request_id,
        worker_id: holder.id,
        outcome: SourcePublicationOutcome::Published,
    };
    let uri = format!("/api/v1/worker/source-publication-inspections/{request_id}/report");
    assert_eq!(
        json_call(
            &h.app,
            "POST",
            &uri,
            Some(common::WORKER_KEY),
            Some(serde_json::to_value(report)?)
        )
        .await?
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(waiting.await??, SourcePublicationOutcome::Published);
    Ok(())
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
async fn an_uncertain_source_publication_exposes_its_retained_result() -> anyhow::Result<()> {
    let h = harness().await?;
    std::fs::create_dir_all(h._dirs.0.path().join("output"))?;
    std::fs::write(h._dirs.0.path().join("output/result.mkv"), b"result")?;
    let worker = repository::register(&h.pool, "uncertain-worker").await?;
    let job = repository::insert_job(&h.pool, "source.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = repository::claim(&h.pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    let lease = forgearr::domain::LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    repository::begin_source_publication(
        &h.pool,
        job.id,
        &lease,
        repository::PublishedOutput {
            name: "result.mkv",
            sha256: &"bb".repeat(32),
            bytes: 6,
            warning: false,
        },
        forgearr::domain::LibraryOutputPolicy::ReplaceInPlace,
    )
    .await?;
    repository::mark_source_publication_uncertain(&h.pool, job.id, lease.lease_epoch).await?;
    let uri = format!("/api/v1/jobs/{}/output", job.id);
    let response = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), JSON_LIMIT).await?.as_ref(),
        b"result"
    );
    let (status, _) = json_call(&h.app, "DELETE", &uri, Some(common::ADMIN_KEY), None).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!h._dirs.0.path().join("output/result.mkv").exists());
    assert!(
        repository::downloadable_output_name(&h.pool, job.id)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn retention_and_reception_routes_are_scoped_to_the_worker_and_its_lease()
-> anyhow::Result<()> {
    use forgearr_shared::transfer::ArtifactDeclaration;
    use forgearr_shared::transfer::ArtifactKind::Working;
    let h = harness().await?;
    let owner =
        register_worker_with_transfer(&h, "detenteur", "http://detenteur.lan:9000/").await?;
    let taker = register_worker_with_encoders(&h, "traitant", &[]).await?;
    install_transfer_keys(&h).await?;
    repository::insert_job(&h.pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = claim_for(&h, owner)
        .await?
        .ok_or_else(|| anyhow::anyhow!("le détenteur doit obtenir la tâche"))?;
    let decl = ArtifactDeclaration {
        root: "/work".into(),
        name: "handoff-file".into(),
        sha256: "ab".repeat(32),
        bytes: 7,
    };
    let status = declare_artifact_call(&h, &claim, OWNER_KEY, owner, Working, &decl).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Seul le détenteur interroge la rétention de son fichier, à l'époque de son bail.
    let job = claim.job_id;
    let epoch = claim.lease_epoch;
    let (st, retained) = retained_call(&h, job, OWNER_KEY, owner, epoch).await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(retained, true);
    let (_, other_epoch) = retained_call(&h, job, OWNER_KEY, owner, epoch + 1).await?;
    assert_eq!(other_epoch, false);
    let (st, _) = retained_call(&h, job, TARGET_KEY, owner, epoch).await?;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Le relais libère le bail ; le repreneur reçoit le fichier de travail et un billet.
    let target = serde_json::json!({"mode": "worker", "worker_id": taker});
    let response = relay_call(&h, &claim, owner, target).await?;
    assert_eq!(response["relayed"], true);
    let taken = claim_for(&h, taker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la cible doit obtenir la tâche"))?;
    assert!(taken.handoff_input.is_some());
    assert!(taken.source_ticket.is_some());
    let (_, still) = retained_call(&h, job, OWNER_KEY, owner, epoch).await?;
    assert_eq!(still, true);

    // Seul le repreneur confirme la réception ; la confirmation est répétable.
    let status = received_call(&h, &taken, OWNER_KEY, taker).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let status = received_call(&h, &taken, TARGET_KEY, taker).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let status = received_call(&h, &taken, TARGET_KEY, taker).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, released) = retained_call(&h, job, OWNER_KEY, owner, epoch).await?;
    assert_eq!(released, false);
    Ok(())
}

/// Le central réclame-t-il encore le fichier de travail retenu par ce worker à cette époque ?
async fn retained_call(
    h: &Harness,
    job: Uuid,
    key: &str,
    worker: Uuid,
    epoch: i64,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    let query = format!("worker_id={worker}&lease_epoch={epoch}");
    let uri = format!("/api/v1/jobs/{job}/retained/working?{query}");
    json_call(&h.app, "GET", &uri, Some(key), None).await
}

/// Le repreneur confirme la réception du fichier de travail avec son bail.
async fn received_call(
    h: &Harness,
    claim: &Claim,
    key: &str,
    worker: Uuid,
) -> anyhow::Result<StatusCode> {
    let uri = format!("/api/v1/jobs/{}/artifacts/working/received", claim.job_id);
    let request = leased_request("POST", uri, claim, key, worker).body(Body::empty())?;
    Ok(h.app.clone().oneshot(request).await?.status())
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

/// Enregistre un worker qui annonce un serveur de transfert.
async fn register_worker_with_transfer(
    h: &Harness,
    name: &str,
    transfer_url: &str,
) -> anyhow::Result<Uuid> {
    let (st, worker) = json_call(
        &h.app,
        "POST",
        "/api/v1/workers",
        Some(common::WORKER_KEY),
        Some(serde_json::json!({"name": name, "transfer_url": transfer_url})),
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        worker["transfer_enabled"],
        forgearr::repository::valid_transfer_url(transfer_url).is_some()
    );
    Ok(Uuid::parse_str(worker["id"].as_str().unwrap_or_default())?)
}

/// Bibliothèque dont `owner` détient le dossier (accès direct) et que `target` traite par transfert,
/// puis une tâche de cette bibliothèque relayée de `owner` vers `target`.
async fn relayed_job_from(
    h: &Harness,
    owner: Uuid,
    target: Uuid,
) -> anyhow::Result<forgearr::domain::Job> {
    let library = repository::insert_library(
        &h.pool,
        forgearr::domain::LibraryWrite {
            name: format!("Films {owner}"),
            workers: vec![forgearr::domain::LibraryWorkerPath {
                source_key: Some("nas".into()),
                worker_id: owner,
                local_path: "/mnt/owner".into(),
                access: forgearr::domain::LibraryAccess::Read,
            }],
            transfer_workers: vec![target],
            ..Default::default()
        },
    )
    .await?;
    let job = repository::insert_job_with_options(
        &h.pool,
        "film.mkv",
        None,
        &"ee".repeat(32),
        SOURCE_CONTENT.len() as i64,
        Some(library.id),
        None,
    )
    .await?;
    sqlx::query(
        "UPDATE jobs SET source_key='nas', source_root='/mnt/owner', source_worker_id=?1 WHERE id=?2",
    )
    .bind(owner)
    .bind(job.id)
    .execute(&h.pool)
    .await?;
    sqlx::query(
        "INSERT INTO job_source_mounts(job_id, worker_id, local_path, writable) VALUES(?1, ?2, '/mnt/owner', 0)",
    )
    .bind(job.id)
    .bind(owner)
    .execute(&h.pool)
    .await?;
    let claim = claim_for(h, owner)
        .await?
        .ok_or_else(|| anyhow::anyhow!("le détenteur doit obtenir la tâche"))?;
    assert!(
        claim.source_ticket.is_none(),
        "le détenteur lit sa propre source"
    );
    let response = relay_call(
        h,
        &claim,
        owner,
        serde_json::json!({"mode": "worker", "worker_id": target}),
    )
    .await?;
    assert_eq!(response["relayed"], true);
    Ok(job)
}

const OWNER_KEY: &str = "fdf_d0000001_abcdef0123456789abcdef0123456789abcdef0123456789";
const TARGET_KEY: &str = "fdf_d0000002_89abcdef0123456789abcdef0123456789abcdef01234567";

/// Clés nommées comme les workers « detenteur » et « traitant » : une clé n'agit que pour son worker.
async fn install_transfer_keys(h: &Harness) -> anyhow::Result<()> {
    use forgearr::api_keys::install_known_key;
    use forgearr_shared::Role;
    install_known_key(&h.pool, OWNER_KEY, "detenteur", Role::Worker).await?;
    install_known_key(&h.pool, TARGET_KEY, "traitant", Role::Worker).await?;
    Ok(())
}

/// Le worker source (ou un imposteur) présente un billet au central.
async fn redeem_call(
    h: &Harness,
    key: &str,
    worker: Uuid,
    token: &str,
) -> anyhow::Result<(StatusCode, serde_json::Value)> {
    json_call(
        &h.app,
        "POST",
        "/api/v1/worker/transfers/redeem",
        Some(key),
        Some(serde_json::json!({"token": token, "worker_id": worker})),
    )
    .await
}

/// Requête d'un worker qui présente sa clé et son bail.
fn leased_request(
    method: &str,
    uri: String,
    claim: &Claim,
    key: &str,
    worker: Uuid,
) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {key}"))
        .header("x-worker-id", worker.to_string())
        .header("x-lease-token", claim.lease_token.to_string())
        .header("x-lease-epoch", claim.lease_epoch.to_string())
}

/// Le worker traitant redemande un billet frais avec son bail.
async fn reissue_ticket_call(
    h: &Harness,
    claim: &Claim,
    key: &str,
    worker: Uuid,
) -> anyhow::Result<(StatusCode, Option<forgearr_shared::transfer::SourceTicket>)> {
    let uri = format!("/api/v1/jobs/{}/source/ticket", claim.job_id);
    let request = leased_request("POST", uri, claim, key, worker).body(Body::empty())?;
    let response = h.app.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), JSON_LIMIT).await?;
    let ticket = serde_json::from_slice::<Option<forgearr_shared::transfer::SourceTicket>>(&bytes);
    Ok((status, ticket.ok().flatten()))
}

/// Le détenteur du bail déclare un artefact qu'il retient.
async fn declare_artifact_call(
    h: &Harness,
    claim: &Claim,
    key: &str,
    worker: Uuid,
    kind: forgearr_shared::transfer::ArtifactKind,
    declaration: &forgearr_shared::transfer::ArtifactDeclaration,
) -> anyhow::Result<StatusCode> {
    let uri = format!("/api/v1/jobs/{}/artifacts/{}", claim.job_id, kind.as_str());
    let request = leased_request("PUT", uri, claim, key, worker)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(declaration)?))?;
    Ok(h.app.clone().oneshot(request).await?.status())
}

#[tokio::test]
async fn a_fresh_ticket_can_be_reissued_with_the_lease_and_replaces_the_previous_one()
-> anyhow::Result<()> {
    let h = harness().await?;
    let owner =
        register_worker_with_transfer(&h, "detenteur", "http://detenteur.lan:9000/").await?;
    let target = register_worker_with_encoders(&h, "traitant", &[]).await?;
    relayed_job_from(&h, owner, target).await?;
    install_transfer_keys(&h).await?;
    let claim = claim_for(&h, target)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la cible doit obtenir la tâche"))?;
    let first = claim
        .source_ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("un billet est attendu"))?;

    let (st, second) = reissue_ticket_call(&h, &claim, TARGET_KEY, target).await?;
    assert_eq!(st, StatusCode::OK);
    let second = second.ok_or_else(|| anyhow::anyhow!("un billet frais est attendu"))?;
    assert_eq!(second.url, first.url);
    assert_ne!(second.token, first.token);
    let (st, _) = redeem_call(&h, OWNER_KEY, owner, &first.token).await?;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "l'ancien billet est remplacé");
    let (st, _) = redeem_call(&h, OWNER_KEY, owner, &second.token).await?;
    assert_eq!(st, StatusCode::OK);

    // Un billet expiré se remplace aussi : la reprise d'un long transfert ne dépend pas de la durée.
    sqlx::query("UPDATE source_tickets SET expires_at=datetime('now', '-1 minute')")
        .execute(&h.pool)
        .await?;
    let (st, third) = reissue_ticket_call(&h, &claim, TARGET_KEY, target).await?;
    assert_eq!(st, StatusCode::OK);
    let third = third.ok_or_else(|| anyhow::anyhow!("un billet frais est attendu"))?;
    let (st, _) = redeem_call(&h, OWNER_KEY, owner, &third.token).await?;
    assert_eq!(st, StatusCode::OK);

    let (st, _) = reissue_ticket_call(&h, &claim, OWNER_KEY, target).await?;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "la clé d'un autre worker ne peut pas demander un billet"
    );
    let stale = Claim {
        lease_token: Uuid::new_v4(),
        ..claim
    };
    let (st, ticket) = reissue_ticket_call(&h, &stale, TARGET_KEY, target).await?;
    assert!(
        st.is_client_error() && ticket.is_none(),
        "bail inconnu refusé"
    );
    Ok(())
}

#[tokio::test]
async fn a_lease_holder_declares_retained_artifacts_that_a_taker_can_get_tickets_for()
-> anyhow::Result<()> {
    use forgearr_shared::transfer::ArtifactDeclaration;
    use forgearr_shared::transfer::ArtifactKind::{self, Result as ResultFile, Working};
    let h = harness().await?;
    let owner =
        register_worker_with_transfer(&h, "detenteur", "http://detenteur.lan:9000/").await?;
    let taker = register_worker_with_encoders(&h, "traitant", &[]).await?;
    install_transfer_keys(&h).await?;
    repository::insert_job(&h.pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = claim_for(&h, owner)
        .await?
        .ok_or_else(|| anyhow::anyhow!("le détenteur doit obtenir la tâche"))?;
    let job = claim.job_id;
    let decl = ArtifactDeclaration {
        root: "/work/retained".into(),
        name: "working.mkv".into(),
        sha256: "ab".repeat(32),
        bytes: 7,
    };
    let issue = |kind: ArtifactKind, taker: Uuid| {
        repository::issue_artifact_ticket(&h.pool, job, kind, taker, claim.lease_epoch)
    };

    // Sans artefact déclaré, pas de billet.
    assert!(issue(Working, taker).await?.is_none());

    // Seule la clé du worker qui détient le bail déclare, et seulement un fichier valide.
    let status = declare_artifact_call(&h, &claim, TARGET_KEY, owner, Working, &decl).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let stale = Claim {
        lease_token: Uuid::new_v4(),
        ..claim.clone()
    };
    let status = declare_artifact_call(&h, &stale, OWNER_KEY, owner, Working, &decl).await?;
    assert!(status.is_client_error());
    let invalid = ArtifactDeclaration {
        name: "../working.mkv".into(),
        ..decl.clone()
    };
    let status = declare_artifact_call(&h, &claim, OWNER_KEY, owner, Working, &invalid).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let none_yet = repository::get_artifact(&h.pool, job, Working).await?;
    assert!(none_yet.is_none());

    let status = declare_artifact_call(&h, &claim, OWNER_KEY, owner, Working, &decl).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stored = repository::get_artifact(&h.pool, job, Working)
        .await?
        .ok_or_else(|| anyhow::anyhow!("l'artefact doit être enregistré"))?;
    assert_eq!(stored.holder_worker_id, owner);
    assert_eq!(stored.lease_epoch, claim.lease_epoch);
    assert_eq!(stored.declaration, decl);

    // Le détenteur n'a pas besoin de tirer son propre fichier ; le résultat n'est pas déclaré.
    assert!(issue(Working, owner).await?.is_none());
    assert!(issue(ResultFile, taker).await?.is_none());
    let ticket = issue(Working, taker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("un billet d'artefact est attendu"))?;
    assert_eq!(ticket.url, "http://detenteur.lan:9000");
    let (st, grant) = redeem_call(&h, OWNER_KEY, owner, &ticket.token).await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(grant["root"], "/work/retained");
    assert_eq!(grant["source_name"], "working.mkv");
    assert_eq!(grant["bytes"], 7);
    assert_eq!(grant["sha256"], "ab".repeat(32));

    // Un billet de résultat coexiste avec celui du fichier de travail pour le même bail.
    let result = ArtifactDeclaration {
        name: "result.mkv".into(),
        ..decl.clone()
    };
    let status = declare_artifact_call(&h, &claim, OWNER_KEY, owner, ResultFile, &result).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let result_ticket = issue(ResultFile, taker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("un billet de résultat est attendu"))?;
    let (st, grant) = redeem_call(&h, OWNER_KEY, owner, &result_ticket.token).await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(grant["source_name"], "result.mkv");
    let (st, _) = redeem_call(&h, OWNER_KEY, owner, &ticket.token).await?;
    assert_eq!(st, StatusCode::OK);

    // Redéclarer remplace l'artefact ; le libérer le retire.
    let renamed = ArtifactDeclaration {
        name: "working-2.mkv".into(),
        ..decl.clone()
    };
    let status = declare_artifact_call(&h, &claim, OWNER_KEY, owner, Working, &renamed).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, grant) = redeem_call(&h, OWNER_KEY, owner, &ticket.token).await?;
    assert_eq!(grant["source_name"], "working-2.mkv");
    assert!(repository::release_artifact(&h.pool, job, Working).await?);
    assert!(!repository::release_artifact(&h.pool, job, Working).await?);
    let (st, _) = redeem_call(&h, OWNER_KEY, owner, &ticket.token).await?;
    assert_eq!(st, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn a_relayed_job_gets_a_ticket_that_only_the_source_worker_can_redeem() -> anyhow::Result<()>
{
    let h = harness().await?;
    let owner =
        register_worker_with_transfer(&h, "detenteur", "http://detenteur.lan:9000/").await?;
    let target = register_worker_with_encoders(&h, "traitant", &[]).await?;
    let job = relayed_job_from(&h, owner, target).await?;

    let claim = claim_for(&h, target)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la cible doit obtenir la tâche"))?;
    assert_eq!(claim.job_id, job.id);
    let ticket = claim
        .source_ticket
        .ok_or_else(|| anyhow::anyhow!("un billet est attendu"))?;
    assert_eq!(ticket.url, "http://detenteur.lan:9000");

    install_transfer_keys(&h).await?;
    let (st, grant) = redeem_call(&h, OWNER_KEY, owner, &ticket.token).await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(grant["root"], "/mnt/owner");
    assert_eq!(grant["source_name"], "film.mkv");
    assert_eq!(grant["bytes"], SOURCE_CONTENT.len());
    let (st, _) = redeem_call(&h, TARGET_KEY, target, &ticket.token).await?;
    assert_eq!(
        st,
        StatusCode::UNAUTHORIZED,
        "seul le worker source valide le billet"
    );
    let (st, _) = redeem_call(&h, OWNER_KEY, owner, "inconnu").await?;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = redeem_call(&h, TARGET_KEY, owner, &ticket.token).await?;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "le preneur du billet ne peut pas se faire passer pour le worker source"
    );
    let (st, _) = redeem_call(&h, common::WORKER_KEY, owner, &ticket.token).await?;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "une clé de worker n'agit pas pour un autre worker"
    );
    Ok(())
}

#[tokio::test]
async fn a_worker_without_transfer_url_gets_no_ticket_and_an_invalid_url_is_ignored()
-> anyhow::Result<()> {
    let h = harness().await?;
    let owner = register_worker_with_transfer(&h, "detenteur", "ftp://nope").await?;
    let target = register_worker_with_encoders(&h, "traitant", &[]).await?;
    relayed_job_from(&h, owner, target).await?;
    let claim = claim_for(&h, target)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la cible doit obtenir la tâche"))?;
    assert!(
        claim.source_ticket.is_none(),
        "repli par le central seulement"
    );
    Ok(())
}

#[tokio::test]
async fn the_central_relays_a_worker_held_source_without_storing_it() -> anyhow::Result<()> {
    let h = harness().await?;
    let owner = register_worker_with_encoders(&h, "detenteur", &[]).await?;
    let target = register_worker_with_encoders(&h, "traitant", &[]).await?;
    let job = relayed_job_from(&h, owner, target).await?;
    let claim = claim_for(&h, target)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la cible doit obtenir la tâche"))?;

    // Le worker traitant demande la source au central ; le détenteur l'envoie en parallèle.
    let download = {
        let app = h.app.clone();
        let request = Request::builder()
            .uri(format!("/api/v1/jobs/{}/source", job.id))
            .header("authorization", format!("Bearer {}", common::WORKER_KEY))
            .header("x-worker-id", target.to_string())
            .header("x-lease-token", claim.lease_token.to_string())
            .header("x-lease-epoch", claim.lease_epoch.to_string())
            .body(Body::empty())?;
        tokio::spawn(async move {
            let response = app.oneshot(request).await?;
            let status = response.status();
            let body = to_bytes(response.into_body(), JSON_LIMIT).await?;
            anyhow::Ok((status, body))
        })
    };
    let (st, tasks) = json_call(
        &h.app,
        "GET",
        &format!("/api/v1/worker/source-relay/pending?worker_id={owner}&wait_ms=5000"),
        Some(common::WORKER_KEY),
        None,
    )
    .await?;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(tasks[0]["root"], "/mnt/owner");
    assert_eq!(tasks[0]["source_name"], "film.mkv");
    let request_id = tasks[0]["request_id"].as_str().unwrap_or_default();
    let send = Request::builder()
        .method("PUT")
        .uri(format!(
            "/api/v1/worker/source-relay/{request_id}?worker_id={owner}"
        ))
        .header("authorization", format!("Bearer {}", common::WORKER_KEY))
        .body(Body::from(SOURCE_CONTENT))?;
    assert_eq!(
        h.app.clone().oneshot(send).await?.status(),
        StatusCode::NO_CONTENT
    );
    let (status, body) = download.await??;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), SOURCE_CONTENT);
    Ok(())
}

#[tokio::test]
async fn the_holder_gets_a_fresh_ticket_to_pull_the_result_from_the_worker_that_retains_it()
-> anyhow::Result<()> {
    use forgearr::domain::{LeaseRequest, LibraryOutputPolicy};
    use forgearr_shared::transfer::{
        ArtifactDeclaration, ArtifactKind::Result as ResultFile, SourcePublicationOutcome,
        SourcePublicationReport, SourcePublicationTask, SourceTicket,
    };
    use std::time::Duration;

    let h = harness().await?;
    let holder = register_worker_with_encoders(&h, "test-worker", &[]).await?;
    let processor =
        register_worker_with_transfer(&h, "detenteur", "http://detenteur.lan:9000/").await?;
    install_transfer_keys(&h).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = claim_for(&h, processor)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    let decl = ArtifactDeclaration {
        root: "/work".into(),
        name: "result-file.mkv".into(),
        sha256: "bb".repeat(32),
        bytes: 6,
    };
    let status = declare_artifact_call(&h, &claim, OWNER_KEY, processor, ResultFile, &decl).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let task = SourcePublicationTask {
        request_id: Uuid::new_v4(),
        worker_id: holder,
        root: "/media".into(),
        source_name: "film.mkv".into(),
        source_sha256: "aa".repeat(32),
        source_bytes: 10,
        result_sha256: decl.sha256.clone(),
        result_bytes: decl.bytes,
        extension: "mkv".into(),
        policy: LibraryOutputPolicy::ReplaceWithBackup,
        result_ticket: None,
    };
    let request_id = task.request_id;
    let lease = LeaseRequest {
        worker_id: processor,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let broker = h.publication.clone();
    let waiting = tokio::spawn(async move {
        broker
            .request_pulled(task, job.id, lease, Duration::from_secs(5))
            .await
    });
    let ticket_uri =
        format!("/api/v1/worker/source-publications/{request_id}/ticket?worker_id={holder}");

    // Tant que la demande n'a pas été remise au détenteur, personne n'obtient de billet.
    let (status, _) = json_call(&h.app, "POST", &ticket_uri, Some(common::WORKER_KEY), None).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let pending =
        format!("/api/v1/worker/source-publications/pending?worker_id={holder}&wait_ms=3000");
    let (status, tasks) =
        json_call(&h.app, "GET", &pending, Some(common::WORKER_KEY), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks[0]["request_id"], request_id.to_string());

    // Seul le détenteur désigné, sous la clé de son worker, obtient un billet valable chez le
    // worker qui retient le résultat.
    let (status, _) = json_call(&h.app, "POST", &ticket_uri, Some(OWNER_KEY), None).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) =
        json_call(&h.app, "POST", &ticket_uri, Some(common::WORKER_KEY), None).await?;
    assert_eq!(status, StatusCode::OK);
    let ticket: SourceTicket = serde_json::from_value(body)?;
    assert_eq!(ticket.url, "http://detenteur.lan:9000");
    let (status, grant) = redeem_call(&h, OWNER_KEY, processor, &ticket.token).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(grant["source_name"], "result-file.mkv");
    let (status, _) = redeem_call(&h, OWNER_KEY, holder, &ticket.token).await?;
    assert_ne!(status, StatusCode::OK, "le billet n'est pas pour le détenteur");

    // Le détenteur termine : la demande aboutit.
    let report = SourcePublicationReport {
        request_id,
        worker_id: holder,
        outcome: SourcePublicationOutcome::Published,
    };
    let uri = format!("/api/v1/worker/source-publications/{request_id}/report");
    let body = serde_json::to_value(report)?;
    let (status, _) =
        json_call(&h.app, "POST", &uri, Some(common::WORKER_KEY), Some(body)).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let outcome = waiting.await??;
    assert!(matches!(outcome, SourcePublicationOutcome::Published));
    Ok(())
}

#[tokio::test]
async fn source_publication_routes_require_the_holder_and_active_lease() -> anyhow::Result<()> {
    use forgearr::domain::{LeaseRequest, LibraryOutputPolicy};
    use forgearr_shared::transfer::{
        SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
    };
    use std::time::Duration;

    let h = harness().await?;
    let holder = register_worker_with_encoders(&h, "test-worker", &[]).await?;
    let other = register_worker_with_encoders(&h, "autre", &[]).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = claim_for(&h, other)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    sqlx::query(
        "UPDATE jobs SET source_root='/media', source_worker_id=?1, source_writable=1 WHERE id=?2",
    )
    .bind(holder)
    .bind(job.id)
    .execute(&h.pool)
    .await?;
    let staged = h._dirs.0.path().join("result.mkv");
    std::fs::write(&staged, b"result")?;
    let task = SourcePublicationTask {
        request_id: Uuid::new_v4(),
        worker_id: holder,
        root: "/media".into(),
        source_name: "film.mkv".into(),
        source_sha256: "aa".repeat(32),
        source_bytes: 10,
        result_sha256: "bb".repeat(32),
        result_bytes: 6,
        extension: "mkv".into(),
        policy: LibraryOutputPolicy::ReplaceWithBackup,
        result_ticket: None,
    };
    let request_id = task.request_id;
    let lease = LeaseRequest {
        worker_id: other,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let broker = h.publication.clone();
    let waiting = tokio::spawn(async move {
        broker
            .request(task, staged, job.id, lease, Duration::from_secs(5))
            .await
    });
    let wrong = format!("/api/v1/worker/source-publications/pending?worker_id={other}");
    assert_eq!(
        json_call(&h.app, "GET", &wrong, Some(common::WORKER_KEY), None)
            .await?
            .0,
        StatusCode::FORBIDDEN
    );
    let pending =
        format!("/api/v1/worker/source-publications/pending?worker_id={holder}&wait_ms=3000");
    let (status, tasks) =
        json_call(&h.app, "GET", &pending, Some(common::WORKER_KEY), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks[0]["request_id"], request_id.to_string());
    let uri = format!("/api/v1/worker/source-publications/{request_id}/staged?worker_id={holder}");
    let response = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header("authorization", format!("Bearer {}", common::WORKER_KEY))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), JSON_LIMIT).await?.as_ref(),
        b"result"
    );
    let authorize =
        format!("/api/v1/worker/source-publications/{request_id}/authorize?worker_id={holder}");
    assert_eq!(
        json_call(&h.app, "GET", &authorize, Some(common::WORKER_KEY), None)
            .await?
            .0,
        StatusCode::NO_CONTENT
    );
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 minute') WHERE id=?1")
        .bind(claim.job_id)
        .execute(&h.pool)
        .await?;
    assert_eq!(
        json_call(&h.app, "GET", &uri, Some(common::WORKER_KEY), None)
            .await?
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        json_call(&h.app, "GET", &authorize, Some(common::WORKER_KEY), None)
            .await?
            .0,
        StatusCode::CONFLICT
    );
    let report = SourcePublicationReport {
        request_id,
        worker_id: holder,
        outcome: SourcePublicationOutcome::SourceChanged,
    };
    let uri = format!("/api/v1/worker/source-publications/{request_id}/report");
    assert_eq!(
        json_call(
            &h.app,
            "POST",
            &uri,
            Some(common::WORKER_KEY),
            Some(serde_json::to_value(&report)?)
        )
        .await?
        .0,
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '+2 minutes') WHERE id=?1")
        .bind(claim.job_id)
        .execute(&h.pool)
        .await?;
    assert_eq!(
        json_call(
            &h.app,
            "POST",
            &uri,
            Some(common::WORKER_KEY),
            Some(serde_json::to_value(report)?)
        )
        .await?
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(waiting.await??, SourcePublicationOutcome::SourceChanged);
    Ok(())
}

#[tokio::test]
async fn software_encoder_probe_route_requires_worker_auth_and_returns_plugin_contract()
-> anyhow::Result<()> {
    let h = harness().await?;
    let root = h._dirs.2.path();
    let mut index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("index.json"))?)?;
    index["entries"].as_array_mut().ok_or_else(|| anyhow::anyhow!("index invalide"))?.push(serde_json::json!({"id":"probe-app","type":"app","version":"1.0.0","name":"Probe","manifestPath":"apps/probe-app/manifest.json"}));
    std::fs::create_dir_all(root.join("apps/probe-app"))?;
    std::fs::write(root.join("index.json"), serde_json::to_vec(&index)?)?;
    std::fs::write(
        root.join("apps/probe-app/manifest.json"),
        serde_json::to_vec(
            &serde_json::json!({"schemaVersion":"1.1.0","id":"probe-app","type":"app","version":"1.0.0","name":"Probe"}),
        )?,
    )?;
    std::fs::write(
        root.join("apps/probe-app/distribution.v1.json"),
        serde_json::to_vec(
            &serde_json::json!({"toolId":"probe-app","toolVersion":"1.0.0","releaseStatus":"released","artifacts":[{"arch":"x86_64","archiveUrl":"https://example.org/a.tar.gz","archiveFormat":"tar.gz","archiveSha256":"a".repeat(64),"binaryRelativePath":"probe","binarySha256":"b".repeat(64)}]}),
        )?,
    )?;
    let manifest_path = root.join("plugins/core/manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    manifest["files"].as_array_mut().ok_or_else(|| anyhow::anyhow!("manifeste invalide"))?.push(serde_json::json!({"path":"probe.json","role":"capability-probe","mediaType":"application/json"}));
    std::fs::write(manifest_path, serde_json::to_vec(&manifest)?)?;
    std::fs::write(
        root.join("plugins/core/probe.json"),
        serde_json::to_vec(
            &serde_json::json!({"schemaVersion":"1.0.0","toolId":"probe-app","args":["-encoders"],"encoders":[{"encoder":"libx265","codec":"hevc"}]}),
        )?,
    )?;
    let uri = "/api/v1/worker/software-encoder-probes";
    assert_eq!(
        json_call(&h.app, "GET", uri, None, None).await?.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, probes) = json_call(&h.app, "GET", uri, Some(common::WORKER_KEY), None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(probes[0]["toolId"], "probe-app");
    assert_eq!(probes[0]["sha256"], "b".repeat(64));
    assert_eq!(probes[0]["encoders"][0]["codec"], "hevc");
    Ok(())
}

#[tokio::test]
async fn a_relay_requiring_cpu_can_be_claimed_by_a_software_encoder() -> anyhow::Result<()> {
    let h = harness().await?;
    let origin = register_worker_with_encoders(&h, "origine-cpu", &[]).await?;
    let target = register_worker_with_encoders(&h, "encodeur-cpu", &[]).await?;
    sqlx::query("UPDATE workers SET telemetry_json=json_set(telemetry_json, '$.capabilities.softwareEncoders', json(?2)) WHERE id=?1")
        .bind(target).bind("[\"hevc\"]").execute(&h.pool).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"bb".repeat(32), 512).await?;
    let claim = claim_for(&h, origin)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail manquant"))?;
    let response = relay_call(
        &h,
        &claim,
        origin,
        serde_json::json!({"mode":"capable","requirement":{"codec":"hevc","acceleration":"cpu"}}),
    )
    .await?;
    assert_eq!(response["relayed"], true);
    let resumed = claim_for(&h, target)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise CPU manquante"))?;
    assert_eq!(resumed.job_id, job.id);
    Ok(())
}

#[tokio::test]
async fn cpu_in_hardware_telemetry_does_not_satisfy_a_software_requirement() -> anyhow::Result<()> {
    let h = harness().await?;
    let origin = register_worker_with_encoders(&h, "origine-fausse-capacite", &[]).await?;
    let target = register_worker_with_encoders(&h, "fausse-capacite", &[]).await?;
    sqlx::query("UPDATE workers SET telemetry_json=json_set(telemetry_json, '$.capabilities.hardwareEncoders', json(?2)) WHERE id=?1")
        .bind(target).bind(r#"[{"codec":"hevc","acceleration":"cpu"}]"#).execute(&h.pool).await?;
    let job = repository::insert_job(&h.pool, "film.mkv", None, &"bb".repeat(32), 512).await?;
    sqlx::query("INSERT INTO job_relays(job_id,from_worker_id,relay_count,resume_node_id,requirement_json) VALUES(?1,?2,1,'next',?3)")
        .bind(job.id).bind(origin).bind(r#"{"codec":"hevc","acceleration":"cpu"}"#).execute(&h.pool).await?;
    assert!(claim_for(&h, target).await?.is_none());
    sqlx::query("UPDATE workers SET telemetry_json=json_set(telemetry_json, '$.capabilities.softwareEncoders', json(?2)) WHERE id=?1")
        .bind(target).bind(r#"["hevc"]"#).execute(&h.pool).await?;
    assert_eq!(
        claim_for(&h, target).await?.map(|claim| claim.job_id),
        Some(job.id)
    );
    Ok(())
}
