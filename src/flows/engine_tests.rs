//! Tests du moteur de flux 100% déclaratif, avec contrats JSON et faux exécutables.

use super::*;
use crate::{tool_run::test_support::fake_tool, tools_cache::host_arch};
use serde_json::json;
use std::sync::{Arc, Mutex};

const INSPECT_JSON: &str = r#"{"container":{"recognized":true,"supported":true},"tracks":[{"type":"video","codec":"HEVC"}]}"#;
/// Dernier argument = fichier de sortie, créé non vide ; progression fractionnelle sur stderr.
const FAKE_WRITER: &str = r#"for a; do last=$a; done; printf '\000\000\000\030ftypisom\000\000\002\000' > "$last"; printf '(40/100)\r' >&2"#;

pub(super) fn install_fake_tool(
    cache: &ToolCacheManager,
    tool_id: &str,
    version: &str,
    name: &str,
    script: &str,
) -> std::io::Result<()> {
    let dir = cache.tool_dir(tool_id, version, host_arch());
    std::fs::create_dir_all(dir.join("bin"))?;
    std::fs::write(dir.join(".ready"), "")?;
    fake_tool(&dir.join("bin"), name, script)?;
    Ok(())
}

pub(super) fn install_fake_probe(cache: &ToolCacheManager) -> std::io::Result<()> {
    install_fake_tool(
        cache,
        "mkvtoolnix",
        "1.0.0",
        "mkvmerge",
        &format!("echo '{INSPECT_JSON}'"),
    )
}

pub(super) fn base_registry() -> OperationRegistry {
    let mut reg = OperationRegistry::new();
    for op in [
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.core.file-input",
            "version": "1.0.0",
            "name": "Entrée",
            "providerId": "essentiel",
            "providerKind": "plugin",
            "capability": "control",
            "status": "validated",
            "runtime": { "kind": "primitive", "primitive": "entry" }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.core.file-output",
            "version": "1.0.0",
            "name": "Sortie",
            "providerId": "essentiel",
            "providerKind": "plugin",
            "capability": "control",
            "status": "validated",
            "runtime": { "kind": "primitive", "primitive": "terminal" }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.mkvtoolnix.probe",
            "version": "1.0.0",
            "name": "Probe",
            "providerId": "video",
            "providerKind": "plugin",
            "capability": "probe",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "mkvtoolnix", "version": "1.0.0" },
                "executable": "mkvmerge",
                "executionMode": "probe-primary",
                "probeFormat": "json",
                "probeRequirePaths": ["container.recognized", "tracks"],
                "argv": ["--output-charset", "UTF-8", "-J", { "param": "source", "as": "filePath" }],
                "console": { "successExitCodes": [0, 1], "warningExitCodes": [1] }
            }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.mp4box.remux",
            "version": "1.0.0",
            "name": "MP4Box",
            "providerId": "video",
            "providerKind": "plugin",
            "capability": "remux",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "mp4box", "version": "1.0.0" },
                "executable": "MP4Box",
                "executionMode": "staged-output",
                "outputExtension": "mp4",
                "outputContentType": "video/mp4",
                "outputFilename": "remuxed.mp4",
                "postValidate": "magic-bytes",
                "postValidateMagic": { "offset": 4, "ascii": "ftyp" },
                "errorCode": "remux_failed",
                "defaults": { "faststart": true, "custom_parameters": "" },
                "customArgsForbidden": ["-add", "-cat", "-new", "-out"],
                "argv": [
                    "-add", { "param": "source", "as": "filePath" },
                    { "when": { "param": "faststart", "truthy": true }, "then": ["-inter", "500"], "else": ["-flat"] },
                    { "param": "custom_parameters", "as": "safeCustomArgs" },
                    "-new", { "param": "destination", "as": "filePath" }
                ],
                "console": { "progressFormat": "fraction-100", "progressStream": "stderr", "successExitCodes": [0] }
            }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.ffmpeg.video.encode",
            "version": "1.0.0",
            "name": "FFmpeg",
            "providerId": "video",
            "providerKind": "plugin",
            "capability": "encode",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "ffmpeg", "version": "1.0.0" },
                "executable": "ffmpeg",
                "executionMode": "staged-output",
                "outputExtension": "mkv",
                "outputContentType": "video/x-matroska",
                "outputFilename": "encoded.mkv",
                "postValidate": "valid-probe",
                "errorCode": "encode_failed",
                "successMessage": "encodage terminé",
                "defaults": { "codec": "h265", "encoder": "nvidia", "quality": 6 },
                "derived": [
                    { "target": "video_encoder", "kind": "map", "inputs": ["codec", "encoder"], "table": { "h265|nvidia": "hevc_nvenc", "h265|*": "libx265" }, "default": "libx265" }
                ],
                "argv": ["-i", { "param": "source", "as": "filePath" }, "-c:v", { "param": "video_encoder" }, { "param": "destination", "as": "filePath" }],
                "retryFallback": [{ "when": { "param": "encoder", "in": ["nvidia"] }, "overrides": { "encoder": "cpu" }, "message": "encodage terminé (repli matériel -> CPU)" }],
                "console": { "progressFormat": "elapsed-time", "progressStream": "stderr", "successExitCodes": [0] }
            }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.mkvtoolnix.mkvpropedit",
            "version": "1.0.0",
            "name": "mkvpropedit",
            "providerId": "video",
            "providerKind": "plugin",
            "capability": "remux",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "mkvtoolnix", "version": "1.0.0" },
                "executable": "mkvpropedit",
                "executionMode": "in-place",
                "outputProfile": "mkv_metadata_edit",
                "skipWhenUnchanged": true,
                "errorCode": "remux_failed",
                "defaults": { "default_audio_lang": "", "force_subtitles": false },
                "argv": [
                    { "param": "source", "as": "filePath" },
                    { "when": { "param": "default_audio_lang", "nonEmpty": true }, "then": ["--edit", "track:a1", "--set", { "format": "language={default_audio_lang}" }] }
                ],
                "console": { "successExitCodes": [0, 1], "warningExitCodes": [1] }
            }
        }),
    ] {
        if let Err(err) = reg.register_json(op) {
            panic!("valid fixture operation: {err}");
        }
    }
    for (block, op) in [
        ("file-input", "operation.core.file-input"),
        ("essentiel.file-input", "operation.core.file-input"),
        ("file-output", "operation.core.file-output"),
        ("essentiel.file-output", "operation.core.file-output"),
        ("video.remux-mp4", "operation.mp4box.remux"),
        ("video.encode", "operation.ffmpeg.video.encode"),
        (
            "block.video.mkvpropedit",
            "operation.mkvtoolnix.mkvpropedit",
        ),
    ] {
        reg.register_block_mapping(block, op);
    }
    reg
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

type ProgressLog = Arc<Mutex<Vec<(u32, String)>>>;

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
async fn remux_mp4_contract_runs_mp4box_and_reports_progress()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;
    install_fake_tool(&cache, "mp4box", "1.0.0", "MP4Box", FAKE_WRITER)?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let reg = base_registry();
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "video.remux-mp4", "name": "mp4"},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "n3"}]
    }));
    let http = reqwest::Client::new();
    let (progress, log) = recording_progress();
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;
    assert_eq!(outcome.final_file, work.join("remuxed.mp4"));
    assert_eq!(outcome.output_spec.extension, "mp4");
    assert_eq!(outcome.output_spec.content_type, "video/mp4");
    assert!(outcome.final_file.is_file() && !outcome.is_in_place);
    assert!(
        log.lock()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|(percent, _)| *percent == 40)
    );
    Ok(())
}

#[tokio::test]
async fn encode_contract_applies_gpu_to_cpu_retry_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;
    install_fake_tool(
        &cache,
        "ffmpeg",
        "1.0.0",
        "ffmpeg",
        r#"
for a; do last=$a; done
case "$*" in
  *hevc_nvenc*) echo "NVENC unavailable" >&2; exit 1 ;;
  *libx265*) : > "$last"; printf 'ok' > "$last"; exit 0 ;;
esac
exit 2
"#,
    )?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let reg = base_registry();
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "video.encode", "name": "enc", "config": {"codec": "h265", "encoder": "nvidia"}},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "n3"}]
    }));
    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;
    assert_eq!(outcome.output_spec.extension, "mkv");
    assert_eq!(outcome.output_spec.content_type, "video/x-matroska");
    assert!(
        outcome
            .steps
            .iter()
            .any(|s| s.message.as_deref() == Some("encodage terminé (repli matériel -> CPU)"))
    );
    Ok(())
}

#[tokio::test]
async fn unknown_third_party_plugin_operation_runs_from_contract_alone()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;
    install_fake_tool(
        &cache,
        "app.custom-brand",
        "2.0.0",
        "brandctl",
        r#"
case "$*" in
  *--label=FLUX*) for a; do last=$a; done; printf 'stamped' > "$last"; exit 0 ;;
esac
exit 9
"#,
    )?;
    let mut reg = base_registry();
    reg.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": "operation.custom.brand",
        "version": "1.0.0",
        "name": "Marquage tiers",
        "providerId": "plugin.custom",
        "providerKind": "plugin",
        "capability": "transform",
        "status": "validated",
        "runtime": {
            "kind": "binary",
            "tool": { "id": "app.custom-brand", "version": "2.0.0" },
            "executable": "brandctl",
            "executionMode": "staged-output",
            "outputExtension": "mkv",
            "outputContentType": "video/x-matroska",
            "outputFilename": "branded.mkv",
            "postValidate": "valid-probe",
            "successMessage": "marquage tiers réussi",
            "defaults": { "label": "FLUX" },
            "argv": [
                { "format": "--label={label}" },
                { "param": "source", "as": "filePath" },
                { "param": "destination", "as": "filePath" }
            ],
            "console": { "successExitCodes": [0] }
        }
    }))?;
    reg.register_block_mapping("block.custom.brand", "operation.custom.brand");

    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "block.custom.brand", "name": "brand"},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "n3"}]
    }));
    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;
    assert_eq!(outcome.final_file, work.join("branded.mkv"));
    assert_eq!(std::fs::read_to_string(&outcome.final_file)?, "stamped");
    Ok(())
}

#[tokio::test]
async fn mkvpropedit_skips_when_unchanged_and_fails_cleanly_on_error()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let reg = base_registry();
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "block.video.mkvpropedit", "name": "edit", "config": {}},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "n3"}]
    }));
    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    // mkvpropedit binaire absent du cache mais aucune modif demandée -> ignoré sans erreur
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, dir.path(), progress)
    .await
    .map_err(|e| e.message)?;
    assert!(
        outcome
            .steps
            .iter()
            .any(|s| s.message.as_deref() == Some("aucune modification demandée"))
    );
    Ok(())
}

fn register_core_and_probe(reg: &mut OperationRegistry) -> Result<(), Box<dyn std::error::Error>> {
    for op in [
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.core.file-input",
            "version": "1.0.0",
            "name": "Entrée",
            "providerId": "essentiel",
            "providerKind": "plugin",
            "capability": "control",
            "status": "validated",
            "runtime": { "kind": "primitive", "primitive": "entry" }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.core.file-output",
            "version": "1.0.0",
            "name": "Sortie",
            "providerId": "essentiel",
            "providerKind": "plugin",
            "capability": "control",
            "status": "validated",
            "runtime": { "kind": "primitive", "primitive": "terminal" }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.mkvtoolnix.probe",
            "version": "1.0.0",
            "name": "Probe",
            "providerId": "video",
            "providerKind": "plugin",
            "capability": "probe",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "mkvtoolnix", "version": "1.0.0" },
                "executable": "mkvmerge",
                "executionMode": "probe-primary",
                "probeFormat": "json",
                "probeRequirePaths": ["container.recognized", "tracks"],
                "argv": ["--output-charset", "UTF-8", "-J", { "param": "source", "as": "filePath" }],
                "console": { "successExitCodes": [0, 1], "warningExitCodes": [1] }
            }
        }),
    ] {
        reg.register_json(op)?;
    }
    reg.register_block_mapping("file-input", "operation.core.file-input");
    reg.register_block_mapping("file-output", "operation.core.file-output");
    Ok(())
}

#[tokio::test]
async fn local_script_executes_end_to_end_across_registry_routing_and_claim_payload()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::JobStepStatus;
    use crate::plugins::save_local_script;
    use forgearr_shared::local::{ScriptInterpreter, UpsertLocalScriptRequest};

    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;

    save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            id: None,
            name: "Router".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: concat!(
                "cat >/dev/null\n",
                "printf 'transformed-payload' > transformed.mkv\n",
                "printf '{\"status\":\"ok\",\"workingFile\":\"transformed.mkv\",\"nextPort\":\"alt\",\"mediaInfo\":{\"customTag\":\"from-script\"},\"message\":\"routé\"}\n'",
            )
            .into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;
    save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            id: None,
            name: "Skipper".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: concat!(
                "if grep -q '\"customTag\":\"from-script\"'; then\n",
                "  printf '{\"status\":\"skip\",\"message\":\"mediaInfo reçu\"}\n'\n",
                "else\n",
                "  printf '{\"status\":\"error\",\"message\":\"mediaInfo absent\"}\n'\n",
                "fi\n",
            )
            .into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;
    save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            id: None,
            name: "Unused".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code:
                "cat >/dev/null\nprintf '{\"status\":\"error\",\"message\":\"mauvaise branche\"}'"
                    .into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;

    let mut reg = OperationRegistry::load_from_roots(&[local_dir]);
    register_core_and_probe(&mut reg)?;

    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "raw-input")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "block.script.router", "name": "Router"},
            {"id": "n3", "type": "block.script.skipper", "name": "Skipper"},
            {"id": "n_bad", "type": "block.script.unused", "name": "Unused"},
            {"id": "n4", "type": "file-output", "name": "out"}
        ],
        "connections": [
            {"from": "n1", "to": "n2"},
            {"from": "n2", "to": "n_bad", "port": "main"},
            {"from": "n2", "to": "n3", "port": "alt"},
            {"from": "n3", "to": "n4"}
        ]
    }));

    let (claim_ops, claim_blocks) = reg.claim_payload_for_flow(Some(&flow));
    assert_eq!(
        claim_blocks.get("block.script.router").map(String::as_str),
        Some("operation.script.router")
    );
    assert!(
        claim_ops
            .get("operation.script.router")
            .and_then(|v| v.get("runtime"))
            .and_then(|v| v.get("code"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|code| code.contains("transformed.mkv"))
    );

    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;

    assert_eq!(
        outcome.final_file,
        std::fs::canonicalize(work.join("transformed.mkv"))?
    );
    assert_eq!(
        std::fs::read_to_string(&outcome.final_file)?,
        "transformed-payload"
    );
    assert!(outcome.steps.iter().any(|s| {
        s.node_id == "n3"
            && s.status == JobStepStatus::Skipped
            && s.message.as_deref() == Some("mediaInfo reçu")
    }));
    Ok(())
}

#[tokio::test]
async fn local_script_stop_pipeline_halts_execution_before_output()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::plugins::save_local_script;
    use forgearr_shared::local::{ScriptInterpreter, UpsertLocalScriptRequest};

    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;

    save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            id: None,
            name: "Halt".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: "cat >/dev/null\nprintf '{\"status\":\"ok\",\"stopPipeline\":true,\"message\":\"arrêt demandé\"}'"
                .into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;

    let mut reg = OperationRegistry::load_from_roots(&[local_dir]);
    register_core_and_probe(&mut reg)?;

    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "raw")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "block.script.halt", "name": "Halt"},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [
            {"from": "n1", "to": "n2"},
            {"from": "n2", "to": "n3"}
        ]
    }));

    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;

    assert_eq!(outcome.steps.len(), 2);
    assert_eq!(outcome.steps[1].node_id, "n2");
    assert_eq!(outcome.steps[1].message.as_deref(), Some("arrêt demandé"));
    Ok(())
}

/// Enregistre le bloc unifié `scripts.run` (interpréteur `sh`).
pub(super) fn register_scripts_run(
    reg: &mut OperationRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    reg.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": "operation.scripts.run",
        "version": "1.0.0",
        "name": "Script",
        "providerId": "scripts",
        "providerKind": "plugin",
        "capability": "script",
        "status": "validated",
        "runtime": {
            "kind": "script",
            "tool": { "id": "interpreteurs", "version": "1.0.0" },
            "interpreter": "sh",
            "timeoutSeconds": 300
        }
    }))?;
    reg.register_block_mapping("scripts.run", "operation.scripts.run");
    Ok(())
}

#[tokio::test]
async fn unified_script_block_supports_inline_library_ref_and_strict_next_port()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::plugins::save_local_script;
    use forgearr_shared::local::{ScriptInterpreter, UpsertLocalScriptRequest};

    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;

    save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            id: None,
            name: "Lib Branch".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: "cat >/dev/null\nprintf '{\"status\":\"ok\",\"nextPort\":\"false\",\"message\":\"branche fausse\"}'"
                .into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;

    let mut reg = OperationRegistry::load_from_roots(&[local_dir]);
    register_core_and_probe(&mut reg)?;
    register_scripts_run(&mut reg)?;

    // 1. Mode inline -> passe au bloc suivant en mode library (scriptId) qui renvoie nextPort: "false".
    // Seule la branche "true" est câblée : next_connection ne doit PAS retomber sur "true".
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {
                "id": "n2",
                "type": "scripts.run",
                "name": "Inline",
                "config": {
                    "mode": "inline",
                    "interpreter": "sh",
                    "code": "cat >/dev/null\nprintf '{\"status\":\"ok\",\"message\":\"inline ok\"}'"
                }
            },
            {
                "id": "n3",
                "type": "scripts.run",
                "name": "Library Ref",
                "config": {
                    "mode": "library",
                    "scriptId": "script.lib-branch",
                    "routing": "conditional"
                }
            },
            {"id": "n_true", "type": "file-output", "name": "out"}
        ],
        "connections": [
            {"from": "n1", "to": "n2"},
            {"from": "n2", "to": "n3"},
            {"from": "n3", "to": "n_true", "port": "true"}
        ]
    }));

    let (claim_ops, _) = reg.claim_payload_for_flow(Some(&flow));
    assert!(claim_ops.contains_key("operation.scripts.run"));
    assert!(claim_ops.contains_key("operation.script.lib-branch"));

    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "raw")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let http = reqwest::Client::new();
    let (progress, _) = recording_progress();
    let outcome = FlowEngine {
        registry: &reg,
        cache_mgr: &cache,
        http_client: &http,
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: None,
        allow_private_integrations: None,
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo::default(),
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;

    // n1 -> n2 -> n3 exécutés ; n_true non exécuté car nextPort="false" et seul "true" était relié.
    assert_eq!(outcome.steps.len(), 3);
    assert_eq!(outcome.steps[1].message.as_deref(), Some("inline ok"));
    assert_eq!(outcome.steps[2].message.as_deref(), Some("branche fausse"));
    Ok(())
}

#[test]
fn path_tools_count_as_required_tools_of_the_flow() -> Result<(), Box<dyn std::error::Error>> {
    let mut reg = base_registry();
    reg.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": "operation.vmaf.target",
        "version": "1.0.0",
        "name": "Encodage à qualité cible",
        "providerId": "vmaf",
        "providerKind": "plugin",
        "capability": "encode",
        "status": "validated",
        "runtime": {
            "kind": "binary",
            "tool": { "id": "ab-av1", "version": "1.0.0" },
            "pathTools": [
                { "id": "ffmpeg-vmaf", "version": "1.0.0" },
                { "id": "ab-av1", "version": "1.0.0" }
            ],
            "executable": "ab-av1",
            "executionMode": "staged-output",
            "argv": [{ "param": "source", "as": "filePath" }]
        }
    }))?;
    reg.register_block_mapping("vmaf.target", "operation.vmaf.target");
    let flow = flow_record(json!({
        "nodes": [{ "id": "n1", "type": "vmaf.target", "name": "AV1" }],
        "connections": []
    }));
    let ids: Vec<String> = reg
        .required_tool_refs_for_flow(Some(&flow))
        .into_iter()
        .map(|tool| tool.id)
        .collect();
    assert!(ids.contains(&"ab-av1".to_owned()));
    assert!(ids.contains(&"ffmpeg-vmaf".to_owned()));
    assert_eq!(ids.iter().filter(|id| *id == "ab-av1").count(), 1);
    Ok(())
}

/// Sonde factice : refuse les archives, reconnaît le reste.
const PROBE_REFUSING_ARCHIVES: &str = r#"for a; do last=$a; done
case "$last" in *.rar) echo 'conteneur non reconnu' >&2; exit 2;; esac"#;

fn archive_flow_registry(
    accepts_non_media: bool,
) -> Result<OperationRegistry, Box<dyn std::error::Error>> {
    let mut reg = OperationRegistry::new();
    register_core_and_probe(&mut reg)?;
    reg.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": "operation.archives.extract",
        "version": "1.0.0",
        "name": "Extraire une archive",
        "providerId": "archives",
        "providerKind": "plugin",
        "capability": "archive",
        "status": "validated",
        "runtime": {
            "kind": "script",
            "interpreter": "sh",
            "acceptsNonMedia": accepts_non_media,
            "code": "cat >/dev/null\nprintf video > film.mkv\nprintf '{\"status\":\"ok\",\"workingFile\":\"film.mkv\"}'"
        }
    }))?;
    reg.register_block_mapping("archives.extract", "operation.archives.extract");
    Ok(reg)
}

#[tokio::test]
async fn non_media_input_is_accepted_then_the_extracted_file_is_probed()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_tool(
        &cache,
        "mkvtoolnix",
        "1.0.0",
        "mkvmerge",
        &format!("{PROBE_REFUSING_ARCHIVES}\necho '{INSPECT_JSON}'"),
    )?;
    let source = dir.path().join("sortie.rar");
    std::fs::write(&source, b"Rar!")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "archives.extract", "name": "extraire"},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [{"from": "n1", "to": "n2"}, {"from": "n2", "to": "n3"}]
    }));
    let http = reqwest::Client::new();
    let run = |reg: OperationRegistry| {
        let cache = &cache;
        let http = &http;
        let flow = &flow;
        let source = &source;
        let work = &work;
        async move {
            let (progress, _) = recording_progress();
            FlowEngine {
                registry: &reg,
                cache_mgr: cache,
                http_client: http,
                secrets_dir: None,
                plugin_configs: &HashMap::new(),
                tool_versions: &HashMap::new(),
                media_root: None,
                logical_source_name: None,
                allow_private_integrations: None,
                worker_capabilities: None,
                resume_node_id: None,
                run_info: FlowRunInfo::default(),
            }
            .execute_flow(Uuid::new_v4(), flow, source, work, progress)
            .await
        }
    };

    let outcome = run(archive_flow_registry(true)?)
        .await
        .map_err(|err| err.message)?;
    assert_eq!(outcome.final_file, work.join("film.mkv"));

    let refused = run(archive_flow_registry(false)?).await;
    assert!(
        refused.is_err(),
        "sans acceptsNonMedia, la sonde initiale doit échouer"
    );
    Ok(())
}

#[tokio::test]
async fn arr_notification_is_deferred_until_publication() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cache = ToolCacheManager::new(dir.path().join("cache"));
    install_fake_probe(&cache)?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, b"movie")?;
    let mut registry = base_registry();
    registry.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": "operation.sonarr.rescan",
        "version": "1.0.0",
        "name": "Rescan",
        "providerId": "sonarr",
        "providerKind": "plugin",
        "capability": "integration",
        "status": "validated",
        "runtime": {"kind":"http","steps":[{"url":"https://example.invalid/not-used","method":"POST"}]}
    }))?;
    registry.register_block_mapping("sonarr.rescan", "operation.sonarr.rescan");
    let flow = flow_record(json!({
        "nodes":[
            {"id":"entry","type":"file-input","name":"Input"},
            {"id":"notify","type":"sonarr.rescan","name":"Rescan","config":{"after_publication":true}},
            {"id":"end","type":"file-output","name":"Output"}
        ],
        "connections":[{"from":"entry","to":"notify"},{"from":"notify","to":"end"}]
    }));
    let resumed = serde_json::Map::from_iter([("x".to_owned(), json!("1"))]);
    let engine = FlowEngine {
        registry: &registry,
        cache_mgr: &cache,
        http_client: &reqwest::Client::new(),
        secrets_dir: None,
        plugin_configs: &HashMap::new(),
        tool_versions: &HashMap::new(),
        media_root: None,
        logical_source_name: Some("series/episode.mkv"),
        allow_private_integrations: Some(false),
        worker_capabilities: None,
        resume_node_id: None,
        run_info: FlowRunInfo {
            resume_variables: Some(&resumed),
            ..FlowRunInfo::default()
        },
    };
    let outcome = engine
        .execute_flow(
            Uuid::new_v4(),
            &flow,
            &source,
            dir.path(),
            recording_progress().0,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.message))?;
    assert!(outcome.steps.iter().any(|step| {
        step.node_id == "notify"
            && step.status == crate::domain::JobStepStatus::Skipped
            && step.message.as_deref() == Some(publication::DEFERRED_MESSAGE)
            && step
                .variables
                .as_ref()
                .is_some_and(|vars| vars.get("x") == Some(&json!("1")))
    }));
    Ok(())
}
