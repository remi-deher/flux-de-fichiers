//! `POST /api/v1/local/scripts/trial` : essai d'un script sur le worker intégré (ADR 0031, lot 3a).

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{AppState, router},
    repository,
};
use forgearr_shared::workers::INTEGRATED_WORKER_NAME;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TRIAL_URI: &str = "/api/v1/local/scripts/trial";
const BODY_LIMIT_BYTES: usize = 1024 * 1024;
const OTHER_WORKER: &str = "worker-distant";

async fn central() -> anyhow::Result<(AppState, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let state = installation::blank_state(&dir.path().join("config")).await?;
    common::install_keys(&state.pool).await?;
    Ok((state, dir))
}

async fn register(state: &AppState, name: &str) -> anyhow::Result<Uuid> {
    let worker = repository::register_with_metadata(&state.pool, name, None, None, None).await?;
    Ok(worker.id)
}

async fn trial(
    state: &AppState,
    authorized: bool,
    worker: Uuid,
    code: &str,
) -> anyhow::Result<(StatusCode, Value)> {
    let body = json!({"workerId": worker, "interpreter": "sh", "code": code});
    let mut request = Request::builder()
        .method("POST")
        .uri(TRIAL_URI)
        .header("content-type", "application/json");
    if authorized {
        request = request.header("authorization", format!("Bearer {}", common::ADMIN_KEY));
    }
    let response = router(state.clone())
        .oneshot(request.body(Body::from(body.to_string()))?)
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), BODY_LIMIT_BYTES).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

#[tokio::test]
async fn the_integrated_worker_runs_the_edited_code_and_returns_what_it_saw() -> anyhow::Result<()>
{
    let (state, _dir) = central().await?;
    let worker = register(&state, INTEGRATED_WORKER_NAME).await?;
    let code = "cat >/dev/null\necho '{\"message\":\"bonjour\"}'";
    let (status, body) = trial(&state, true, worker, code).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["exitCode"], 0, "{body}");
    assert_eq!(body["response"]["message"], "bonjour", "{body}");
    assert!(body["stdin"].to_string().contains("workingFile"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_trial_needs_a_key_a_known_worker_and_the_integrated_one() -> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let worker = register(&state, INTEGRATED_WORKER_NAME).await?;
    let (status, _) = trial(&state, false, worker, "true").await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = trial(&state, true, Uuid::new_v4(), "true").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let remote = register(&state, OTHER_WORKER).await?;
    let (status, _) = trial(&state, true, remote, "true").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}
