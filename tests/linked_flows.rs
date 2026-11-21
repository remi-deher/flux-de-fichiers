//! Flux liés (ADR 0022) : figement des sous-flux et du flux d'échec, validation à l'enregistrement.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{AppState, router},
    config::Config,
    domain::UpdateFlowRequest,
    repository,
};
use forgearr_shared::flows::{DefaultFailureFlow, LinkedFlow, LinkedFlowRole};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::sync::Arc;
use tower::ServiceExt;

const MAIN_FLOW: &str = "flow.main";
const SUB_FLOW: &str = "flow.sub";
const FAILURE_FLOW: &str = "flow.echec";
const PLAIN_FLOW: &str = "flow.simple";
const MAX_TEST_BODY_BYTES: usize = 64 * 1024;

async fn setup() -> anyhow::Result<(axum::Router, SqlitePool)> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024,
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
    Ok((app, pool))
}

fn calling(target: &str) -> Value {
    json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "Entrée"},
            {"id": "call", "type": "essentiel.sub-flow", "name": "Sous-flux",
             "config": {"flowId": target}}
        ],
        "connections": [{"from": "in", "to": "call"}]
    })
}

fn plain(name: &str) -> Value {
    json!({"nodes": [{"id": "in", "type": "file-input", "name": name}], "connections": []})
}

async fn save(pool: &SqlitePool, id: &str, definition: &Value) -> anyhow::Result<i64> {
    let request = UpdateFlowRequest {
        name: id.into(),
        description: None,
        file_mask: "*.mkv".into(),
        is_default: false,
        definition: definition.to_string(),
    };
    Ok(repository::save_flow(pool, id, &request).await?.version)
}

async fn job_links(pool: &SqlitePool, flow: &str, seed: u8) -> anyhow::Result<Vec<LinkedFlow>> {
    let job = repository::insert_job_with_priority(
        pool,
        &format!("{flow}-{seed}.mkv"),
        None,
        &format!("{seed:02x}").repeat(32),
        1_000,
        None,
        Some(flow),
        None,
    )
    .await?;
    Ok(repository::frozen_linked_flows(pool, job.id).await?)
}

fn versions(links: &[LinkedFlow]) -> Vec<(LinkedFlowRole, String, i64)> {
    links
        .iter()
        .map(|link| (link.role, link.flow.id.clone(), link.flow.version))
        .collect()
}

#[tokio::test]
async fn sub_flows_and_failure_flow_are_frozen_with_the_job() -> anyhow::Result<()> {
    let (_app, pool) = setup().await?;
    save(&pool, SUB_FLOW, &plain("v1")).await?;
    save(&pool, FAILURE_FLOW, &plain("échec")).await?;
    let mut main = calling(SUB_FLOW);
    main["failureFlowId"] = json!(FAILURE_FLOW);
    save(&pool, MAIN_FLOW, &main).await?;

    let first = job_links(&pool, MAIN_FLOW, 1).await?;
    assert_eq!(
        versions(&first),
        vec![
            (LinkedFlowRole::Failure, FAILURE_FLOW.to_owned(), 1),
            (LinkedFlowRole::SubFlow, SUB_FLOW.to_owned(), 1),
        ]
    );

    // Une nouvelle version du sous-flux ne touche pas la tâche déjà créée.
    assert_eq!(save(&pool, SUB_FLOW, &plain("v2")).await?, 2);
    let second = job_links(&pool, MAIN_FLOW, 2).await?;
    let newest = (LinkedFlowRole::SubFlow, SUB_FLOW.to_owned(), 2);
    assert!(versions(&second).contains(&newest));
    let job_one = sqlx::query_scalar::<_, i64>(
        "SELECT version FROM job_linked_flows WHERE role='sub_flow' ORDER BY version LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(job_one, 1);
    Ok(())
}

#[tokio::test]
async fn the_global_failure_flow_applies_only_without_a_flow_choice() -> anyhow::Result<()> {
    let (_app, pool) = setup().await?;
    save(&pool, FAILURE_FLOW, &plain("échec")).await?;
    save(&pool, PLAIN_FLOW, &plain("simple")).await?;
    let mut own = plain("propre");
    own["failureFlowId"] = json!(PLAIN_FLOW);
    save(&pool, MAIN_FLOW, &own).await?;

    assert!(job_links(&pool, PLAIN_FLOW, 1).await?.is_empty());
    repository::set_default_failure_flow(&pool, Some(FAILURE_FLOW)).await?;
    assert_eq!(
        repository::default_failure_flow(&pool).await?.as_deref(),
        Some(FAILURE_FLOW)
    );
    let global = job_links(&pool, PLAIN_FLOW, 2).await?;
    assert_eq!(
        versions(&global),
        vec![(LinkedFlowRole::Failure, FAILURE_FLOW.to_owned(), 1)]
    );
    let chosen = job_links(&pool, MAIN_FLOW, 3).await?;
    assert_eq!(
        versions(&chosen),
        vec![(LinkedFlowRole::Failure, PLAIN_FLOW.to_owned(), 1)]
    );
    repository::set_default_failure_flow(&pool, None).await?;
    assert_eq!(repository::default_failure_flow(&pool).await?, None);
    Ok(())
}

fn authorized(method: &str, uri: &str, body: &Value) -> anyhow::Result<Request<Body>> {
    Ok(Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body)?))?)
}

async fn put_flow(app: &axum::Router, id: &str, definition: &Value) -> anyhow::Result<StatusCode> {
    let flow = json!({
        "name": id,
        "description": null,
        "file_mask": "*.mkv",
        "is_default": false,
        "definition": definition.to_string()
    });
    let request = authorized("PUT", &format!("/api/v1/flows/{id}"), &flow)?;
    Ok(app.clone().oneshot(request).await?.status())
}

/// Enregistre un flux par l'API et vérifie le statut obtenu.
async fn expect_put(
    app: &axum::Router,
    id: &str,
    definition: &Value,
    expected: StatusCode,
) -> anyhow::Result<()> {
    let status = put_flow(app, id, definition).await?;
    anyhow::ensure!(status == expected, "{id} : {status} au lieu de {expected}");
    Ok(())
}

#[tokio::test]
async fn saving_refuses_sub_flow_cycles_and_unknown_links() -> anyhow::Result<()> {
    let (app, _pool) = setup().await?;
    expect_put(&app, "flow.a", &calling("flow.a"), StatusCode::BAD_REQUEST).await?;
    expect_put(&app, "flow.a", &calling("flow.x"), StatusCode::BAD_REQUEST).await?;
    expect_put(&app, "flow.b", &plain("b"), StatusCode::OK).await?;
    expect_put(&app, "flow.a", &calling("flow.b"), StatusCode::OK).await?;
    // flow.b -> flow.a -> flow.b : refusé.
    expect_put(&app, "flow.b", &calling("flow.a"), StatusCode::BAD_REQUEST).await?;

    let mut unknown_failure = plain("c");
    unknown_failure["failureFlowId"] = json!("flow.absent");
    expect_put(&app, "flow.c", &unknown_failure, StatusCode::BAD_REQUEST).await?;

    let validate = json!({"definition": calling("flow.a"), "flowId": "flow.b"});
    let response = app
        .clone()
        .oneshot(authorized("POST", "/api/v1/flows/validate", &validate)?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let report: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), MAX_TEST_BODY_BYTES).await?)?;
    assert_eq!(report["valid"], false);
    Ok(())
}

#[tokio::test]
async fn the_global_failure_flow_is_read_and_written_through_the_api() -> anyhow::Result<()> {
    let (app, _pool) = setup().await?;
    let uri = "/api/v1/flows/failure-default";
    let unknown = json!({"flowId": "flow.absent"});
    let response = app
        .clone()
        .oneshot(authorized("PUT", uri, &unknown)?)
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    expect_put(&app, FAILURE_FLOW, &plain("échec"), StatusCode::OK).await?;
    let chosen = json!({"flowId": FAILURE_FLOW});
    let response = app
        .clone()
        .oneshot(authorized("PUT", uri, &chosen)?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
        .body(Body::empty())?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), MAX_TEST_BODY_BYTES).await?;
    let current: DefaultFailureFlow = serde_json::from_slice(&body)?;
    assert_eq!(current.flow_id.as_deref(), Some(FAILURE_FLOW));

    let cleared = app
        .clone()
        .oneshot(authorized("PUT", uri, &json!({"flowId": null}))?)
        .await?;
    assert_eq!(cleared.status(), StatusCode::OK);
    Ok(())
}
