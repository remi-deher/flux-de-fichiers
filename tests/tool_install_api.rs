//! Installation d'un outil : le téléchargement va à son terme même si le client coupe la connexion.

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{Router, body::Body, http::Request, routing::get};
use forgearr::{
    api::{AppState, router},
    runtime_settings::RUNTIME_SETTINGS_KEY,
    tools_cache::{ToolCacheManager, host_arch},
};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::Notify;
use tower::ServiceExt;

const TOOL: &str = "demo";
const VERSION: &str = "1.0.0";
const TEST_LIMIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(20);

fn archive() -> anyhow::Result<Vec<u8>> {
    let content = b"#!/bin/sh\nexit 0\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    builder.append_data(&mut header, TOOL, content.as_slice())?;
    Ok(builder.into_inner()?.finish()?)
}

fn write_catalog(root: &Path, archive_url: &str, sha256: &str) -> anyhow::Result<()> {
    let tool_dir = root.join("apps").join(TOOL);
    std::fs::create_dir_all(&tool_dir)?;
    let manifest = format!("apps/{TOOL}/manifest.json");
    std::fs::write(
        root.join("index.json"),
        json!({"schemaVersion": "1.0.0", "entries": [{
            "id": TOOL, "type": "app", "version": VERSION, "name": "Démo", "manifestPath": manifest
        }]})
        .to_string(),
    )?;
    std::fs::write(
        root.join(&manifest),
        json!({"schemaVersion": "1.1.0", "id": TOOL, "type": "app", "version": VERSION, "name": "Démo"})
            .to_string(),
    )?;
    std::fs::write(
        tool_dir.join("distribution.v1.json"),
        json!({
            "schemaVersion": "1.0.0", "toolId": TOOL, "toolVersion": VERSION,
            "distributionMode": "autonomous-binary",
            "artifacts": [{
                "arch": host_arch(), "archiveUrl": archive_url, "archiveFormat": "tar.gz",
                "archiveSha256": sha256, "binaryRelativePath": TOOL
            }]
        })
        .to_string(),
    )?;
    Ok(())
}

#[tokio::test]
async fn a_tool_download_outlives_the_connection_that_asked_for_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let archive = archive()?;
    let sha256 = ToolCacheManager::compute_sha256(&archive);
    let (started, resume) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let (server_started, server_resume) = (started.clone(), resume.clone());
    let server = Router::new().route(
        "/outil.tar.gz",
        get(move || async move {
            server_started.notify_one();
            server_resume.notified().await;
            archive
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/outil.tar.gz", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, server).await;
    });

    let catalog = dir.path().join("catalogue");
    write_catalog(&catalog, &url, &sha256)?;
    let blank = installation::blank_state(dir.path()).await?;
    common::install_keys(&blank.pool).await?;
    let mut config = (*blank.config).clone();
    config.plugins_dir = Some(catalog);
    config.tools_cache_dir = dir.path().join("tools");
    let state = AppState::new(blank.pool, Arc::new(config));
    forgearr::repository::set_setting(
        &state.pool,
        RUNTIME_SETTINGS_KEY,
        &json!({"allow_private_plugin_hosts": true}).to_string(),
    )
    .await?;

    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/tools/{TOOL}/install"))
        .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
        .body(Body::empty())?;
    // Quitter l'attente abandonne la requête, comme un client qui coupe pendant le téléchargement.
    tokio::select! {
        response = router(state.clone()).oneshot(request) => {
            anyhow::bail!("réponse avant la fin du téléchargement : {}", response?.status());
        }
        waited = tokio::time::timeout(TEST_LIMIT, started.notified()) => waited?,
    }
    resume.notify_one();

    let cache = state.tool_cache();
    tokio::time::timeout(TEST_LIMIT, async {
        while !cache.is_ready(TOOL, VERSION, host_arch()) {
            tokio::time::sleep(POLL).await;
        }
    })
    .await?;
    Ok(())
}
