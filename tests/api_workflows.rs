//! Tests d'intégration des workflows métier en Rust pur : Jobs, Workers, Tools et Storage.

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
    domain::{Claim, Failure, Job, LeaseRequest, Worker},
    storage,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

async fn test_app() -> anyhow::Result<(axum::Router, tempfile::TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let input_dir = temp_dir.path().join("input");
    std::fs::create_dir_all(&input_dir)?;
    std::fs::write(input_dir.join("film.mkv"), vec![0u8; 1024])?;

    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: temp_dir.path().to_path_buf(),
        input_dir,
        config_dir: temp_dir.path().join("config"),
        tools_cache_dir: temp_dir.path().join("tools-cache"),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key".into(),
        worker_key: "test-worker-key".into(),
        max_file_bytes: 10_000_000,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    std::fs::create_dir_all(&config.config_dir)?;
    std::fs::create_dir_all(&config.tools_cache_dir)?;

    let pool = SqlitePoolOptions::new()
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    let app = router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    });
    Ok((app, temp_dir))
}

#[tokio::test]
async fn test_job_and_worker_workflow() -> anyhow::Result<()> {
    let (app, _dir) = test_app().await?;

    // 1. Création d'une tâche de remuxage
    let new_job_payload = serde_json::json!({
        "source_name": "film.mkv"
    });
    let req = Request::builder()
        .uri("/api/v1/jobs")
        .method("POST")
        .header("Authorization", "Bearer test-admin-key")
        .header("Content-Type", "application/json")
        .body(Body::from(new_job_payload.to_string()))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let job: Job = serde_json::from_slice(&bytes)?;
    assert_eq!(job.source_name, "film.mkv");
    assert_eq!(job.status, "queued");

    // 2. Enregistrement d'un worker
    let register_payload = serde_json::json!({
        "name": "worker-node-1"
    });
    let req = Request::builder()
        .uri("/api/v1/workers")
        .method("POST")
        .header("Authorization", "Bearer test-worker-key")
        .header("Content-Type", "application/json")
        .body(Body::from(register_payload.to_string()))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let worker: Worker = serde_json::from_slice(&bytes)?;
    assert_eq!(worker.name, "worker-node-1");

    // 3. Réclamation de la tâche par le worker
    let claim_payload = serde_json::json!({
        "worker_id": worker.id
    });
    let req = Request::builder()
        .uri("/api/v1/claims")
        .method("POST")
        .header("Authorization", "Bearer test-worker-key")
        .header("Content-Type", "application/json")
        .body(Body::from(claim_payload.to_string()))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let claim_opt: Option<Claim> = serde_json::from_slice(&bytes)?;
    let Some(claim) = claim_opt else {
        anyhow::bail!("Le bail claim doit être présent");
    };
    assert_eq!(claim.job_id, job.id);

    // 4. Heartbeat périodique sur le bail
    let heartbeat_payload = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        telemetry: None,
    };
    let req = Request::builder()
        .uri(format!("/api/v1/jobs/{}/heartbeat", job.id))
        .method("POST")
        .header("Authorization", "Bearer test-worker-key")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&heartbeat_payload)?))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 5. Signalement d'échec métier avec code d'erreur
    let failure_payload = Failure {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        error_code: "source_integrity".into(),
    };
    let req = Request::builder()
        .uri(format!("/api/v1/jobs/{}/failure", job.id))
        .method("POST")
        .header("Authorization", "Bearer test-worker-key")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&failure_payload)?))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 6. Vérification de l'état final de la tâche via GET
    let req = Request::builder()
        .uri(format!("/api/v1/jobs/{}", job.id))
        .method("GET")
        .header("Authorization", "Bearer test-admin-key")
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let updated_job: Job = serde_json::from_slice(&bytes)?;
    assert_eq!(updated_job.status, "failed");
    assert_eq!(updated_job.error_code.as_deref(), Some("source_integrity"));

    Ok(())
}

#[tokio::test]
async fn test_workers_crud_lifecycle() -> anyhow::Result<()> {
    let (app, _dir) = test_app().await?;

    // Enregistrement d'un worker
    let register_payload = serde_json::json!({ "name": "worker-to-delete" });
    let req = Request::builder()
        .uri("/api/v1/workers")
        .method("POST")
        .header("Authorization", "Bearer test-worker-key")
        .header("Content-Type", "application/json")
        .body(Body::from(register_payload.to_string()))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let worker: Worker = serde_json::from_slice(&bytes)?;

    // Listage des workers
    let req = Request::builder()
        .uri("/api/v1/workers")
        .method("GET")
        .header("Authorization", "Bearer test-admin-key")
        .body(Body::empty())?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let workers: Vec<Worker> = serde_json::from_slice(&bytes)?;
    assert!(workers.iter().any(|w| w.id == worker.id));

    // Suppression du worker
    let req = Request::builder()
        .uri(format!("/api/v1/workers/{}", worker.id))
        .method("DELETE")
        .header("Authorization", "Bearer test-admin-key")
        .body(Body::empty())?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Deuxième suppression -> 404 Not Found
    let req = Request::builder()
        .uri(format!("/api/v1/workers/{}", worker.id))
        .method("DELETE")
        .header("Authorization", "Bearer test-admin-key")
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn test_tools_listing_endpoint() -> anyhow::Result<()> {
    let (app, _dir) = test_app().await?;

    let req = Request::builder()
        .uri("/api/v1/tools")
        .method("GET")
        .header("Authorization", "Bearer test-admin-key")
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let tools: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert!(tools.is_array());
    Ok(())
}

#[tokio::test]
async fn test_storage_list_inputs_behavior() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let input_dir = temp_dir.path();

    // Fichier MKV valide
    let mkv_path = input_dir.join("film_vacances.mkv");
    std::fs::write(&mkv_path, vec![0u8; 1024])?;

    // Fichier non-MKV à ignorer
    let txt_path = input_dir.join("notes.txt");
    std::fs::write(&txt_path, "texte quelconque")?;

    // Sous-répertoire
    let sub_dir = input_dir.join("sous_dossier");
    std::fs::create_dir_all(&sub_dir)?;

    let inputs = storage::list_inputs(input_dir, 10_000_000).await?;
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].name, "film_vacances.mkv");
    assert_eq!(inputs[0].bytes, 1024);

    Ok(())
}
