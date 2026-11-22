//! Conteneur lancé avec la commande `worker` sans enrôlement (ADR 0016). Fichier à part : la
//! réservation vaut pour tout le processus.

#[path = "common/installation.rs"]
mod installation;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{connect_database, router},
    config::Config,
    instance, repository,
};
use forgearr_shared::Role;
use std::path::Path;
use tower::ServiceExt;

const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;

/// Worker demandé dont les médias montés ne permettent de créer aucun dossier.
fn requested_worker_config(dir: &Path, name: &str) -> anyhow::Result<Config> {
    let config_dir = dir.join(name);
    std::fs::create_dir_all(&config_dir)?;
    let blocked = dir.join("medias");
    std::fs::write(&blocked, "x")?;
    let mut config = installation::blank_config(&config_dir)?;
    config.database_url = format!("sqlite://{}", config_dir.join("flux.db").display());
    config.data_dir = blocked.join("data");
    config.input_dir = blocked.join("input");
    config.local_worker = true;
    Ok(config)
}

#[tokio::test]
async fn a_worker_without_enrollment_only_offers_the_worker_setup() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;

    let mut central = requested_worker_config(dir.path(), "central")?;
    let central_pool = connect_database(&central).await?;
    repository::create_first_account(&central_pool, "admin", "empreinte", Role::Admin, None)
        .await?;
    central_pool.close().await;
    assert!(instance::prepare_worker_setup(&mut central).await.is_err());
    assert!(!instance::worker_setup_only());

    let mut worker = requested_worker_config(dir.path(), "worker")?;
    instance::prepare_worker_setup(&mut worker).await?;
    assert!(instance::worker_setup_only());
    assert!(!worker.local_worker);
    for created in [&worker.data_dir, &worker.input_dir] {
        assert!(created.starts_with(&worker.config_dir));
        std::fs::create_dir_all(created)?;
    }

    let state = installation::blank_state(dir.path()).await?;
    let pool = state.pool.clone();
    let app = router(state);

    let request = Request::get("/api/v1/setup/status").body(Body::empty())?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
    let status: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(status["needsSetup"], true);
    assert_eq!(status["instanceKind"], "worker");

    let request = Request::post("/api/v1/setup/central")
        .header("host", "localhost:3000")
        .header("origin", "http://localhost:3000")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"name": "Central", "username": "admin-central", "password": "un-bon-mot-de-passe"})
                .to_string(),
        ))?;
    let response = app.oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(repository::account_count(&pool).await?, 0);
    Ok(())
}
