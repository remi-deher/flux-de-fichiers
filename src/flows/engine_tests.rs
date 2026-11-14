//! Tests du moteur de flux de bout en bout, avec de faux exécutables installés dans un cache d'outils.

use super::*;
use crate::{tool_run::test_support::fake_tool, tools_cache::host_arch};
use std::sync::{Arc, Mutex};

const INSPECT_JSON: &str = r#"{"container":{"recognized":true,"supported":true},"tracks":[{"type":"video","codec":"HEVC"}]}"#;
/// Dernier argument = fichier de sortie, créé vide ; progression sur stderr.
const FAKE_WRITER: &str = r#"for a; do last=$a; done; : > "$last"; printf '(40/100)\r' >&2"#;

/// Installe `script` comme binaire `name` d'un outil « prêt » du cache.
pub(super) fn install_fake_tool(
    cache: &ToolCacheManager,
    (tool_id, version): (&str, &str),
    name: &str,
    script: &str,
) -> std::io::Result<()> {
    let dir = cache.tool_dir(tool_id, version, host_arch());
    std::fs::create_dir_all(dir.join("bin"))?;
    std::fs::write(dir.join(".ready"), "")?;
    fake_tool(&dir.join("bin"), name, script)?;
    Ok(())
}

pub(super) fn install_fake_mkvmerge(cache: &ToolCacheManager) -> std::io::Result<()> {
    install_fake_tool(
        cache,
        (
            crate::plugins::REMUX_TOOL_ID,
            crate::plugins::REMUX_TOOL_VERSION,
        ),
        "mkvmerge",
        &format!("echo '{INSPECT_JSON}'"),
    )
}

pub(super) fn flow_record(definition: serde_json::Value) -> FlowRecord {
    FlowRecord {
        id: "flow".into(),
        name: "test".into(),
        description: None,
        file_mask: "*".into(),
        is_default: false,
        definition: definition.to_string(),
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub(super) type ProgressLog = Arc<Mutex<Vec<(u32, String)>>>;

pub(super) fn recording_progress() -> (FlowProgressCallback, ProgressLog) {
    let log: ProgressLog = Arc::default();
    let sink = log.clone();
    let callback: FlowProgressCallback = Arc::new(move |percent, line| {
        if let Ok(mut log) = sink.lock() {
            log.push((percent, line.to_owned()));
        }
    });
    (callback, log)
}

#[tokio::test]
async fn remux_mp4_block_runs_mp4box_and_reports_progress() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_mkvmerge(&cache)?;
    install_fake_tool(
        &cache,
        (
            crate::plugins::MP4BOX_TOOL_ID,
            crate::plugins::MP4BOX_TOOL_VERSION,
        ),
        "MP4Box",
        FAKE_WRITER,
    )?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let flow = flow_record(serde_json::json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "block.video.remux-mp4", "name": "mp4"},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "n3"}]
    }));
    let http = reqwest::Client::new();
    let (progress, log) = recording_progress();
    let outcome = FlowEngine::new(&cache, &http)
        .execute_flow(&flow, Uuid::new_v4(), &source, &work, progress)
        .await?;
    assert_eq!(outcome.working_file, work.join("remuxed.mp4"));
    assert!(outcome.working_file.is_file() && !outcome.in_place);
    assert!(
        log.lock()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|(percent, _)| *percent == 40)
    );
    Ok(())
}

#[tokio::test]
async fn remux_mp4_block_fails_with_a_typed_error_when_mp4box_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_mkvmerge(&cache)?;
    install_fake_tool(
        &cache,
        (
            crate::plugins::MP4BOX_TOOL_ID,
            crate::plugins::MP4BOX_TOOL_VERSION,
        ),
        "MP4Box",
        "exit 2",
    )?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let flow = flow_record(serde_json::json!({
        "nodes": [{"id": "node-1", "type": "block.video.remux-mp4", "name": "mp4"}],
        "connections": []
    }));
    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    let error = FlowEngine::new(&cache, &http)
        .execute_flow(&flow, Uuid::new_v4(), &source, dir.path(), progress)
        .await
        .err()
        .ok_or("échec attendu")?;
    assert!(matches!(
        error.downcast_ref::<crate::tool_run::ToolRunError>(),
        Some(crate::tool_run::ToolRunError::Failed {
            tool: "mp4box",
            code: Some(2)
        })
    ));
    Ok(())
}
