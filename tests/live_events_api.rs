//! Chaque écriture visible est signalée une fois, après avoir été validée (ADR 0028).

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{AppState, router},
    live::{CHANGE_WINDOW, LiveEvent},
};
use forgearr_shared::live::{ChangeTopic, EVENTS_PATH};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tower::ServiceExt;

const RESPONSE_LIMIT_BYTES: usize = 256 * 1024;
const REPOSITORY: &str = "repo.direct";

struct Harness {
    state: AppState,
    app: axum::Router,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let state = installation::blank_state(dir.path()).await?;
        common::install_keys(&state.pool).await?;
        Ok(Self {
            app: router(state.clone()),
            state,
            _dir: dir,
        })
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |json| Body::from(json.to_string())))?;
        let response = self.app.clone().oneshot(request).await?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), RESPONSE_LIMIT_BYTES).await?;
        anyhow::ensure!(
            status == StatusCode::OK,
            "{method} {path} : {status} {}",
            String::from_utf8_lossy(&bytes)
        );
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Nombre d'événements reçus par sujet pour un appel.
    async fn signalled(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> anyhow::Result<BTreeMap<String, usize>> {
        let mut rx = self.state.event_history.subscribe_from(None).await.rx;
        self.call(method, path, body).await?;
        tokio::time::sleep(CHANGE_WINDOW * 3).await;
        let mut counts = BTreeMap::new();
        while let Ok((_, event)) = rx.try_recv() {
            if let LiveEvent::Changed(topic) = event {
                *counts.entry(format!("{topic:?}")).or_default() += 1;
            }
        }
        Ok(counts)
    }
}

fn once(topics: &[ChangeTopic]) -> BTreeMap<String, usize> {
    topics
        .iter()
        .map(|topic| (format!("{topic:?}"), 1))
        .collect()
}

#[tokio::test]
async fn repository_writes_signal_the_list_and_the_catalog() -> anyhow::Result<()> {
    let harness = Harness::new().await?;
    let both = once(&[ChangeTopic::Catalog, ChangeTopic::Repositories]);
    let toggle = format!("/api/v1/plugins/repositories/{REPOSITORY}/toggle");
    let repository = format!("/api/v1/plugins/repositories/{REPOSITORY}");
    let creation = json!({
        "id": REPOSITORY,
        "name": "Dépôt direct",
        "indexUrl": "https://example.com/index.json"
    });
    let added = harness
        .signalled("POST", "/api/v1/plugins/repositories", Some(creation))
        .await?;
    assert_eq!(added, both);
    assert_eq!(harness.signalled("PUT", &toggle, None).await?, both);
    // Même opération par le MCP, que l'assistant emprunte aussi.
    let by_mcp = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "toggle_repository", "arguments": { "id": REPOSITORY } }
    });
    assert_eq!(harness.signalled("POST", "/mcp", Some(by_mcp)).await?, both);
    assert_eq!(harness.signalled("DELETE", &repository, None).await?, both);
    Ok(())
}

#[tokio::test]
async fn a_failed_repository_removal_leaves_it_in_place_unsignalled() -> anyhow::Result<()> {
    let harness = Harness::new().await?;
    let creation = json!({
        "id": REPOSITORY,
        "name": "Dépôt direct",
        "indexUrl": "https://example.com/index.json"
    });
    harness
        .signalled("POST", "/api/v1/plugins/repositories", Some(creation))
        .await?;
    let pool = &harness.state.pool;
    sqlx::query(
        "INSERT INTO plugin_release_pins (scope, version, sha256, source_url) \
         VALUES (?1, '1.0.0', 'ab', 'https://example.com')",
    )
    .bind(format!("repo:{REPOSITORY}"))
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TRIGGER refuse_unpin BEFORE DELETE ON plugin_release_pins \
         BEGIN SELECT RAISE(ABORT, 'refus'); END",
    )
    .execute(pool)
    .await?;
    let mut rx = harness.state.event_history.subscribe_from(None).await.rx;
    let path = format!("/api/v1/plugins/repositories/{REPOSITORY}");
    assert!(harness.call("DELETE", &path, None).await.is_err());
    tokio::time::sleep(CHANGE_WINDOW * 3).await;
    assert!(rx.try_recv().is_err());
    let listed = harness
        .call("GET", "/api/v1/plugins/repositories", None)
        .await?;
    assert!(listed.to_string().contains(REPOSITORY));
    Ok(())
}

#[tokio::test]
async fn local_library_writes_signal_the_catalog_once_written() -> anyhow::Result<()> {
    let harness = Harness::new().await?;
    let catalog = once(&[ChangeTopic::Catalog]);
    let script = json!({ "name": "Direct", "interpreter": "sh", "code": "true" });
    let created = harness
        .signalled("POST", "/api/v1/local/scripts", Some(script.clone()))
        .await?;
    assert_eq!(created, catalog);
    let listed = harness.call("GET", "/api/v1/plugins", None).await?;
    assert!(listed.to_string().contains("script.direct"));
    let preview = harness
        .call("GET", "/api/v1/catalog-scripts/script.direct", None)
        .await?;
    assert_eq!(preview["inCollection"], true);

    // Écriture refusée : rien à relire.
    let mut rx = harness.state.event_history.subscribe_from(None).await.rx;
    assert!(
        harness
            .call("POST", "/api/v1/local/scripts", Some(script))
            .await
            .is_err()
    );
    tokio::time::sleep(CHANGE_WINDOW * 2).await;
    assert!(rx.try_recv().is_err());

    let settings = json!({ "url": "http://exemple" });
    let saved = harness
        .signalled(
            "PUT",
            "/api/v1/plugins/script.direct/settings",
            Some(settings),
        )
        .await?;
    assert_eq!(saved, once(&[ChangeTopic::Catalog, ChangeTopic::Settings]));

    let deleted = harness
        .signalled("DELETE", "/api/v1/local/items/script.direct", None)
        .await?;
    assert_eq!(deleted, catalog);
    Ok(())
}

#[tokio::test]
async fn a_purge_of_the_central_tools_is_signalled() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config = installation::blank_config(dir.path())?;
    config.tools_cache_dir = dir.path().join("tools");
    std::fs::create_dir_all(config.tools_cache_dir.join("demo/1.0.0"))?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&config.database_url)
        .await?;
    let state = AppState::new(pool, std::sync::Arc::new(config));
    let mut rx = state.event_history.subscribe_from(None).await.rx;
    let keep = [("demo".to_owned(), "2.0.0".to_owned())];
    assert_eq!(state.tool_cache().prune_unused(&keep)?, 1);
    let (_, event) = tokio::time::timeout(CHANGE_WINDOW * 10, rx.recv()).await??;
    assert!(matches!(event, LiveEvent::Changed(ChangeTopic::Tools)));
    Ok(())
}

#[tokio::test]
async fn a_worker_key_cannot_open_the_stream() -> anyhow::Result<()> {
    let harness = Harness::new().await?;
    let response = harness
        .app
        .oneshot(
            Request::builder()
                .uri(EVENTS_PATH)
                .header("authorization", format!("Bearer {}", common::WORKER_KEY))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    Ok(())
}
