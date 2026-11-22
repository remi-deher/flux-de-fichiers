//! Tests du moteur pour l'accord du central avant une action irréversible (ADR 0027).

use super::engine_tests::{
    base_registry, flow_record, install_fake_probe, install_fake_tool, recording_progress,
};
use super::*;
use crate::job_runner::action_gate::{ActionRefusal, TaskStop, test_support::FakeGate};
use serde_json::json;

const CALL_NODE: &str = "appel";

const EDIT_NODE: &str = "edition";

/// Registre complété d'un bloc d'appel HTTP de cette méthode.
fn registry_with_call(method: &str) -> anyhow::Result<OperationRegistry> {
    let mut registry = base_registry();
    registry.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": "operation.test.call",
        "version": "1.0.0",
        "name": "Appel",
        "providerId": "test",
        "providerKind": "plugin",
        "capability": "integration",
        "status": "validated",
        "runtime": {"kind": "http", "steps": [{"url": "https://example.invalid/", "method": method}]}
    }))?;
    registry.register_block_mapping("test.call", "operation.test.call");
    Ok(registry)
}

/// Exécute un flux d'un seul bloc entre l'entrée et la sortie ; `dir` porte source, cache et travail.
async fn run_block(
    dir: &std::path::Path,
    registry: &OperationRegistry,
    block: serde_json::Value,
    gate: &FakeGate,
) -> anyhow::Result<Result<FlowExecutionOutcome, FlowExecutionError>> {
    let cache = ToolCacheManager::new(dir.join("cache"));
    install_fake_probe(&cache)?;
    let source = dir.join("in.mkv");
    std::fs::write(&source, b"x")?;
    let id = block["id"].clone();
    let flow = flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            block,
            {"id": "out", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "in", "to": id}, {"from": id, "to": "out"}]
    }));
    Ok(FlowEngine {
        registry,
        cache_mgr: &cache,
        http_client: &reqwest::Client::new(),
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: Some(false),
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
        resumed_outputs: &HashMap::new(),
        action_gate: Some(gate),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, dir, recording_progress().0)
    .await)
}

/// Exécute un flux dont l'unique bloc est un appel HTTP de cette méthode.
async fn run_call(
    method: &str,
    gate: &FakeGate,
) -> anyhow::Result<Result<FlowExecutionOutcome, FlowExecutionError>> {
    let dir = tempfile::tempdir()?;
    let block = json!({"id": CALL_NODE, "type": "test.call", "name": "appel"});
    run_block(dir.path(), &registry_with_call(method)?, block, gate).await
}

/// Exécute une édition en place par un outil factice ; rend aussi s'il a été lancé.
async fn run_edit(config: serde_json::Value, gate: &FakeGate) -> anyhow::Result<(bool, bool)> {
    let dir = tempfile::tempdir()?;
    let marker = dir.path().join("lance");
    let script = format!(": > '{}'", marker.display());
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_tool(&cache, "mkvtoolnix", "1.0.0", "mkvpropedit", &script)?;
    let block = json!({
        "id": EDIT_NODE, "type": "block.video.mkvpropedit", "name": "edit", "config": config
    });
    let outcome = run_block(dir.path(), &base_registry(), block, gate).await?;
    Ok((outcome.is_ok(), marker.exists()))
}

#[tokio::test]
async fn a_refused_call_stops_the_flow_before_the_request() -> anyhow::Result<()> {
    let gate = FakeGate::new(Err(ActionRefusal::Stop(TaskStop::LeaseLost)));
    assert!(run_call("POST", &gate).await?.is_err());
    assert_eq!(gate.halt(), Some(TaskStop::LeaseLost));
    let call = (IrreversibleAction::HttpCall, CALL_NODE.to_owned());
    assert_eq!(gate.asked(), [call]);
    Ok(())
}

#[tokio::test]
async fn a_read_only_call_asks_for_nothing() -> anyhow::Result<()> {
    let gate = FakeGate::new(Err(ActionRefusal::Excluded("non demandé".into())));
    let outcome = run_call("GET", &gate).await?.map_err(|e| e.message);
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(gate.asked().is_empty());
    Ok(())
}

#[tokio::test]
async fn an_in_place_edit_runs_only_with_the_grant() -> anyhow::Result<()> {
    let change = json!({"default_audio_lang": "fre"});
    let refusing = FakeGate::new(Err(ActionRefusal::Stop(TaskStop::LeaseLost)));
    let (_, launched) = run_edit(change.clone(), &refusing).await?;
    assert!(!launched);
    assert_eq!(refusing.halt(), Some(TaskStop::LeaseLost));
    let edit = (IrreversibleAction::InPlace, EDIT_NODE.to_owned());
    assert_eq!(refusing.asked(), [edit.clone()]);

    let granting = FakeGate::new(Ok(()));
    let (_, launched) = run_edit(change, &granting).await?;
    assert!(launched);
    assert_eq!(granting.asked(), [edit]);
    Ok(())
}

#[tokio::test]
async fn an_edit_without_change_asks_for_nothing() -> anyhow::Result<()> {
    let gate = FakeGate::new(Err(ActionRefusal::Excluded("non demandé".into())));
    let (succeeded, launched) = run_edit(json!({}), &gate).await?;
    assert!(succeeded && !launched);
    assert!(gate.asked().is_empty());
    Ok(())
}
