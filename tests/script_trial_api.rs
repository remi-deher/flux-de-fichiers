//! `POST /api/v1/local/scripts/trial` : essai d'un script sur le worker intégré (ADR 0031, lot 3a) ou sur un
//! worker distant par le courtier (lot 3b).

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

const PENDING_URI: &str = "/api/v1/worker/script-trials/pending";
const REPORT_URI: &str = "/api/v1/worker/script-trials";

async fn send(
    state: &AppState,
    method: &str,
    uri: &str,
    key: Option<&str>,
    body: Option<Value>,
) -> anyhow::Result<(StatusCode, Value)> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(key) = key {
        request = request.header("authorization", format!("Bearer {key}"));
    }
    let body = body.map_or_else(Body::empty, |value| Body::from(value.to_string()));
    let response = router(state.clone()).oneshot(request.body(body)?).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), BODY_LIMIT_BYTES).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

/// Rapport d'essai tel que l'envoie un worker distant.
fn report_body(request_id: &Value, worker: Uuid) -> Value {
    let result = json!({
        "stdin": "{}", "stdout": "vu", "stderr": "", "exitCode": 0, "durationMs": 3,
        "timedOut": false, "truncated": false, "response": null,
        "responseError": null, "launchError": null
    });
    json!({"requestId": request_id, "workerId": worker, "result": result})
}

async fn trial(
    state: &AppState,
    authorized: bool,
    worker: Uuid,
    code: &str,
) -> anyhow::Result<(StatusCode, Value)> {
    let body = json!({"workerId": worker, "interpreter": "sh", "code": code});
    let key = authorized.then_some(common::ADMIN_KEY);
    send(state, "POST", TRIAL_URI, key, Some(body)).await
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
async fn a_trial_needs_a_key_and_a_known_worker() -> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let worker = register(&state, INTEGRATED_WORKER_NAME).await?;
    let (status, _) = trial(&state, false, worker, "true").await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = trial(&state, true, Uuid::new_v4(), "true").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn an_online_remote_worker_runs_the_trial_through_the_relay() -> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let remote = register(&state, OTHER_WORKER).await?;
    let worker = {
        let state = state.clone();
        tokio::spawn(async move {
            let key = Some(common::WORKER_KEY);
            let pending = format!("{PENDING_URI}?worker_id={remote}&wait_ms=3000");
            let (status, tasks) = send(&state, "GET", &pending, key, None).await?;
            anyhow::ensure!(status == StatusCode::OK, "attente refusée : {status}");
            let task = &tasks[0];
            let code = task["request"]["code"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let report = report_body(&task["requestId"], remote);
            let (status, _) = send(&state, "POST", REPORT_URI, key, Some(report)).await?;
            anyhow::ensure!(
                status == StatusCode::NO_CONTENT,
                "rapport refusé : {status}"
            );
            anyhow::Ok(code)
        })
    };
    let (status, body) = trial(&state, true, remote, "echo salut").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stdout"], "vu", "{body}");
    assert_eq!(body["exitCode"], 0, "{body}");
    assert_eq!(worker.await??, "echo salut");
    Ok(())
}

#[tokio::test]
async fn an_offline_remote_worker_is_reported_unavailable() -> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let remote = register(&state, OTHER_WORKER).await?;
    sqlx::query("UPDATE workers SET last_seen=datetime('now', '-1 day') WHERE id=?1")
        .bind(remote)
        .execute(&state.pool)
        .await?;
    let (status, body) = trial(&state, true, remote, "true").await?;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    Ok(())
}

#[tokio::test]
async fn the_relay_routes_are_for_workers_only() -> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let uri = format!("{PENDING_URI}?worker_id={}", Uuid::new_v4());
    let (status, _) = send(&state, "GET", &uri, None, None).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let report = report_body(&json!(Uuid::new_v4()), Uuid::new_v4());
    let (status, _) = send(&state, "POST", REPORT_URI, None, Some(report)).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    Ok(())
}
