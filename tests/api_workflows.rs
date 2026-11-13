//! Tests d'intégration des workflows métier en Rust pur : Jobs, Workers, Tools et Storage.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
    domain::{Claim, Failure, Job, JobErrorCode, JobStatus, LeaseRequest, Worker},
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
    common::install_keys(&pool).await?;
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
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .header("Content-Type", "application/json")
        .body(Body::from(new_job_payload.to_string()))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let job: Job = serde_json::from_slice(&bytes)?;
    assert_eq!(job.source_name, "film.mkv");
    assert_eq!(job.status, JobStatus::Queued);

    // 2. Enregistrement d'un worker
    let register_payload = serde_json::json!({
        "name": "worker-node-1"
    });
    let req = Request::builder()
        .uri("/api/v1/workers")
        .method("POST")
        .header(
            "Authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
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
        .header(
            "Authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
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
        .header(
            "Authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
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
        .header(
            "Authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&failure_payload)?))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 6. Vérification de l'état final de la tâche via GET
    let req = Request::builder()
        .uri(format!("/api/v1/jobs/{}", job.id))
        .method("GET")
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let updated_job: Job = serde_json::from_slice(&bytes)?;
    assert_eq!(updated_job.status, JobStatus::Failed);
    assert_eq!(updated_job.error_code, Some(JobErrorCode::SourceIntegrity));

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
        .header(
            "Authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
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
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
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
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .body(Body::empty())?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Deuxième suppression -> 404 Not Found
    let req = Request::builder()
        .uri(format!("/api/v1/workers/{}", worker.id))
        .method("DELETE")
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
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
        .header(
            "Authorization",
            "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
        )
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

#[tokio::test]
async fn json_routes_reject_oversized_bodies_and_transfers_do_not() -> anyhow::Result<()> {
    use flux_de_fichiers::api::{
        JSON_API_TIMEOUT, JSON_BODY_LIMIT, TRANSFER_TIMEOUT, is_long_running_route,
    };

    assert!(TRANSFER_TIMEOUT > JSON_API_TIMEOUT);
    assert_eq!(JSON_BODY_LIMIT, 16 * 1024);
    assert!(is_long_running_route("/api/v1/jobs/events"));
    assert!(is_long_running_route(
        "/api/v1/jobs/11111111-1111-1111-1111-111111111111/result"
    ));
    assert!(is_long_running_route(
        "/api/v1/jobs/11111111-1111-1111-1111-111111111111/source"
    ));
    assert!(is_long_running_route(
        "/api/v1/jobs/11111111-1111-1111-1111-111111111111/output"
    ));
    assert!(!is_long_running_route("/health"));
    assert!(!is_long_running_route("/api/v1/jobs"));
    assert!(!is_long_running_route(
        "/api/v1/jobs/not-a-transfer/failure"
    ));

    let (app, _dir) = test_app().await?;
    let oversized = "x".repeat(JSON_BODY_LIMIT + 1);
    let req = Request::builder()
        .uri("/api/v1/workers")
        .method("POST")
        .header("content-type", "application/json")
        .header(
            "authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
        .body(Body::from(oversized))?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let req = Request::builder()
        .uri("/api/v1/jobs/11111111-1111-1111-1111-111111111111/result")
        .method("PUT")
        .body(Body::from("x".repeat(JSON_BODY_LIMIT + 1)))?;
    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn failure_rejects_unknown_and_internal_error_codes() -> anyhow::Result<()> {
    let (app, _dir) = test_app().await?;
    for code in ["nope", "lease_expired", "missing_source_fingerprint"] {
        let req = Request::builder()
            .uri("/api/v1/jobs/11111111-1111-1111-1111-111111111111/failure")
            .method("POST")
            .header(
                "authorization",
                "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "worker_id": "22222222-2222-2222-2222-222222222222",
                    "lease_token": "33333333-3333-3333-3333-333333333333",
                    "error_code": code
                })
                .to_string(),
            ))?;
        let resp = app.clone().oneshot(req).await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{code}");
    }
    Ok(())
}
