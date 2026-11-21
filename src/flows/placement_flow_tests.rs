//! Tests du moteur pour les blocs « Capacité du worker » et « Relayer la tâche » (ADR 0018).

use super::engine_tests::{
    base_registry, flow_record, install_fake_probe, install_fake_tool, recording_progress,
};
use super::*;
use forgearr_shared::{
    capabilities::{EncodeAcceleration, HardwareEncoder, VideoCodec},
    relay::RelayTarget,
};
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(super) fn placement_registry() -> OperationRegistry {
    let mut reg = base_registry();
    for (id, primitive) in [
        ("operation.core.worker-capability", "capability"),
        ("operation.core.relay", "relay"),
    ] {
        let registered = reg.register_json(json!({
            "schemaVersion": "1.1.0",
            "id": id,
            "version": "1.0.0",
            "name": id,
            "providerId": "essentiel",
            "providerKind": "plugin",
            "capability": "control",
            "status": "validated",
            "runtime": { "kind": "primitive", "primitive": primitive }
        }));
        assert!(
            registered.is_ok(),
            "contrat de test valide : {registered:?}"
        );
    }
    reg.register_block_mapping("worker-capability", "operation.core.worker-capability");
    reg.register_block_mapping("relay", "operation.core.relay");
    reg
}

/// Entrée -> capacité H.265 ; `capable` -> sortie A ; `incapable` -> relais -> sortie B.
fn relay_flow() -> FlowRecord {
    flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            {"id": "cap", "type": "worker-capability", "name": "cap", "config": {"codec": "hevc"}},
            {"id": "outA", "type": "file-output", "name": "A"},
            {"id": "relay", "type": "relay", "name": "relay", "config": {"mode": "capable", "codec": "hevc"}},
            {"id": "outB", "type": "file-output", "name": "B"}
        ],
        "connections": [
            {"from": "in", "to": "cap"},
            {"from": "cap", "to": "outA", "port": "capable"},
            {"from": "cap", "to": "relay", "port": "incapable"},
            {"from": "relay", "to": "outB"}
        ]
    }))
}

fn hevc_capabilities() -> WorkerCapabilities {
    WorkerCapabilities {
        gpu_models: Vec::new(),
        software_encoders: Vec::new(),
        hardware_encoders: vec![HardwareEncoder {
            codec: VideoCodec::Hevc,
            acceleration: EncodeAcceleration::Nvenc,
        }],
    }
}

async fn run(
    flow: &FlowRecord,
    capabilities: Option<&WorkerCapabilities>,
    resume_node_id: Option<&str>,
) -> Result<FlowExecutionOutcome, String> {
    let registry = placement_registry();
    run_engine(&registry, flow, capabilities, resume_node_id, FlowRunInfo::default()).await
}

/// Exécute `flow` avec de faux outils (sonde, MP4Box, mkvpropedit) et l'identité donnée.
pub(super) async fn run_engine(
    registry: &OperationRegistry,
    flow: &FlowRecord,
    capabilities: Option<&WorkerCapabilities>,
    resume_node_id: Option<&str>,
    run_info: FlowRunInfo<'_>,
) -> Result<FlowExecutionOutcome, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache).map_err(|e| e.to_string())?;
    install_fake_tool(
        &cache,
        "mp4box",
        "1.0.0",
        "MP4Box",
        r#"for a; do last=$a; done; printf '\000\000\000\030ftypisom\000\000\002\000' > "$last""#,
    )
    .map_err(|e| e.to_string())?;
    install_fake_tool(&cache, "mkvtoolnix", "1.0.0", "mkvpropedit", "exit 0")
        .map_err(|e| e.to_string())?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x").map_err(|e| e.to_string())?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work).map_err(|e| e.to_string())?;
    let http = reqwest::Client::new();
    let (progress, _log) = recording_progress();
    FlowEngine {
        registry,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: capabilities,
        resume_node_id,
        run_info,
    }
    .execute_flow(Uuid::new_v4(), flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)
}

fn relay_after(block: &str, config: serde_json::Value) -> FlowRecord {
    flow_record(json!({
        "nodes": [
            {"id":"in", "type":"file-input", "name":"in"},
            {"id":"edit", "type":block, "name":"edit", "config":config},
            {"id":"relay", "type":"relay", "name":"relay", "config":{"mode":"capable", "codec":"hevc"}},
            {"id":"out", "type":"file-output", "name":"out"}
        ],
        "connections": [
            {"from":"in", "to":"edit"}, {"from":"edit", "to":"relay"}, {"from":"relay", "to":"out"}
        ]
    }))
}

#[tokio::test]
async fn relay_accepts_a_staged_working_file_after_processing() -> TestResult {
    let outcome = run(&relay_after("video.remux-mp4", json!({})), None, None).await?;
    assert!(outcome.relay.is_some());
    assert!(outcome.final_file.ends_with("remuxed.mp4"));
    Ok(())
}

#[tokio::test]
async fn relay_refuses_a_source_modified_in_place() -> TestResult {
    let error = run(
        &relay_after(
            "block.video.mkvpropedit",
            json!({"default_audio_lang":"fra"}),
        ),
        None,
        None,
    )
    .await
    .err()
    .ok_or("le transfert doit échouer")?;
    assert!(error.contains("modification de la source en place"));
    Ok(())
}

#[tokio::test]
async fn relay_accepts_an_in_place_operation_that_had_no_changes() -> TestResult {
    let outcome = run(
        &relay_after("block.video.mkvpropedit", json!({})),
        None,
        None,
    )
    .await?;
    assert!(outcome.relay.is_some());
    Ok(())
}

#[tokio::test]
async fn an_incapable_worker_relays_and_stops_locally() -> TestResult {
    let outcome = run(&relay_flow(), None, None).await?;
    let request = outcome.relay.ok_or("un relais est attendu")?;
    assert_eq!(request.resume_node_id, "outB");
    assert!(matches!(request.target, RelayTarget::Capable { .. }));
    let kinds: Vec<_> = outcome
        .steps
        .iter()
        .map(|step| step.block_kind.as_str())
        .collect();
    assert_eq!(kinds, ["file-input", "worker-capability", "relay"]);
    Ok(())
}

#[tokio::test]
async fn a_capable_worker_continues_without_relaying() -> TestResult {
    let outcome = run(&relay_flow(), Some(&hevc_capabilities()), None).await?;
    assert!(outcome.relay.is_none());
    assert_eq!(
        outcome.steps.last().map(|step| step.node_id.as_str()),
        Some("outA")
    );
    Ok(())
}

#[tokio::test]
async fn the_target_worker_resumes_at_the_node_after_the_relay() -> TestResult {
    let outcome = run(&relay_flow(), Some(&hevc_capabilities()), Some("outB")).await?;
    assert!(outcome.relay.is_none());
    let nodes: Vec<_> = outcome
        .steps
        .iter()
        .map(|step| step.node_id.as_str())
        .collect();
    assert_eq!(nodes, ["outB"]);
    Ok(())
}

#[tokio::test]
async fn an_unknown_resume_node_fails_clearly() {
    let error = run(&relay_flow(), None, Some("absent")).await;
    assert!(matches!(error, Err(message) if message.contains("absent")));
}

#[tokio::test]
async fn a_relay_without_resume_output_fails() {
    let flow = flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            {"id": "relay", "type": "relay", "name": "relay", "config": {"mode": "capable", "codec": "hevc"}}
        ],
        "connections": [{"from": "in", "to": "relay"}]
    }));
    let error = run(&flow, None, None).await;
    assert!(matches!(error, Err(message) if message.contains("Reprise")));
}
