//! `POST /api/v1/highlight` : coloration du code en cours d'édition (ADR 0030, lot 2).

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::api::{AppState, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const BODY_LIMIT_BYTES: usize = 1024 * 1024;
const HIGHLIGHT_URI: &str = "/api/v1/highlight";

async fn central() -> anyhow::Result<(AppState, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let state = installation::blank_state(&dir.path().join("config")).await?;
    common::install_keys(&state.pool).await?;
    Ok((state, dir))
}

async fn post(
    state: &AppState,
    authorized: bool,
    body: &Value,
) -> anyhow::Result<(StatusCode, Value)> {
    let mut request = Request::builder()
        .method("POST")
        .uri(HIGHLIGHT_URI)
        .header("content-type", "application/json");
    if authorized {
        request = request.header("authorization", format!("Bearer {}", common::ADMIN_KEY));
    }
    let response = router(state.clone())
        .oneshot(request.body(Body::from(body.to_string()))?)
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), BODY_LIMIT_BYTES * 2).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

#[tokio::test]
async fn highlighting_returns_tokens_per_line_and_plain_text_for_an_unknown_language()
-> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let code = "echo \"a\" # fin\nls\n";

    let (status, body) = post(&state, true, &json!({"code": code, "extension": "sh"})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lines"].as_array().map_or(0, Vec::len), 2, "{body}");
    assert!(body["lines"][0].to_string().contains("comment"), "{body}");

    let unknown = json!({"code": code, "extension": "inconnu"});
    let (status, body) = post(&state, true, &unknown).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(!body["lines"].to_string().contains("comment"), "{body}");
    Ok(())
}

#[tokio::test]
async fn highlighting_requires_a_key_and_rejects_an_oversized_body() -> anyhow::Result<()> {
    let (state, _dir) = central().await?;
    let (status, _) = post(&state, false, &json!({"code": "ls", "extension": "sh"})).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let huge = json!({"code": "a".repeat(BODY_LIMIT_BYTES + 1), "extension": "sh"});
    let (status, _) = post(&state, true, &huge).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}
