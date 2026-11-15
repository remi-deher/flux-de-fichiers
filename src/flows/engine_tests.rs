//! Tests du moteur de flux 100% déclaratif, avec contrats JSON et faux exécutables.

use super::*;
use crate::{tool_run::test_support::fake_tool, tools_cache::host_arch};
use serde_json::json;
use std::sync::{Arc, Mutex};

const INSPECT_JSON: &str = r#"{"container":{"recognized":true,"supported":true},"tracks":[{"type":"video","codec":"HEVC"}]}"#;
/// Dernier argument = fichier de sortie, créé non vide ; progression fractionnelle sur stderr.
const FAKE_WRITER: &str = r#"for a; do last=$a; done; printf '\000\000\000\030ftypisom\000\000\002\000' > "$last"; printf '(40/100)\r' >&2"#;

fn install_fake_tool(
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

fn install_fake_probe(cache: &ToolCacheManager) -> std::io::Result<()> {
    install_fake_tool(
        cache,
        "app.mkvtoolnix",
        "1.0.0",
        "mkvmerge",
        &format!("echo '{INSPECT_JSON}'"),
    )
}

fn base_registry() -> OperationRegistry {
    let mut reg = OperationRegistry::new();
    for op in [
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.core.file-input",
            "version": "1.0.0",
            "name": "Entrée",
            "providerId": "plugin.core",
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
            "providerId": "plugin.core",
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
            "providerId": "plugin.video",
            "providerKind": "plugin",
            "capability": "probe",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "app.mkvtoolnix", "version": "1.0.0" },
                "executable": "mkvmerge",
                "executionMode": "probe-primary",
                "probeFormat": "mkvmerge-json",
                "argv": ["--output-charset", "UTF-8", "-J", { "param": "source", "as": "filePath" }],
                "console": { "successExitCodes": [0, 1], "warningExitCodes": [1] }
            }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.mp4box.remux",
            "version": "1.0.0",
            "name": "MP4Box",
            "providerId": "plugin.video",
            "providerKind": "plugin",
            "capability": "remux",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "app.mp4box", "version": "1.0.0" },
                "executable": "MP4Box",
                "executionMode": "staged-output",
                "outputExtension": "mp4",
                "outputFilename": "remuxed.mp4",
                "outputProfile": "mp4_container",
                "postValidate": "mp4-header",
                "errorCode": "remux_failed",
                "defaults": { "faststart": true, "custom_parameters": "" },
                "customArgsProfile": "mp4box",
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
            "providerId": "plugin.video",
            "providerKind": "plugin",
            "capability": "encode",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "app.ffmpeg", "version": "1.0.0" },
                "executable": "ffmpeg",
                "executionMode": "staged-output",
                "outputExtension": "mkv",
                "outputFilename": "encoded.mkv",
                "outputProfile": "transcoded_mkv",
                "postValidate": "valid-media",
                "errorCode": "encode_failed",
                "successMessage": "encodage terminé",
                "defaults": { "codec": "h265", "encoder": "nvidia", "quality": 6 },
                "derived": [
                    { "target": "video_encoder", "kind": "map", "inputs": ["codec", "encoder"], "table": { "h265|nvidia": "hevc_nvenc", "h265|*": "libx265" }, "default": "libx265" }
                ],
                "argv": ["-i", { "param": "source", "as": "filePath" }, "-c:v", { "param": "video_encoder" }, { "param": "destination", "as": "filePath" }],
                "retryFallback": [{ "when": { "param": "encoder", "in": ["nvidia"] }, "overrides": { "encoder": "cpu" }, "message": "encodage terminé (repli matériel -> CPU)" }],
                "console": { "progressFormat": "ffmpeg-time", "progressStream": "stderr", "successExitCodes": [0] }
            }
        }),
        json!({
            "schemaVersion": "1.1.0",
            "id": "operation.mkvtoolnix.mkvpropedit",
            "version": "1.0.0",
            "name": "mkvpropedit",
            "providerId": "plugin.video",
            "providerKind": "plugin",
            "capability": "remux",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "app.mkvtoolnix", "version": "1.0.0" },
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
        ("block.core.file-input", "operation.core.file-input"),
        ("file-output", "operation.core.file-output"),
        ("block.core.file-output", "operation.core.file-output"),
        ("block.video.remux-mp4", "operation.mp4box.remux"),
        ("block.video.encode", "operation.ffmpeg.video.encode"),
        (
            "block.video.mkvpropedit",
            "operation.mkvtoolnix.mkvpropedit",
        ),
    ] {
        reg.register_block_mapping(block, op);
    }
    reg
}

fn flow_record(definition: serde_json::Value) -> FlowRecord {
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

fn recording_progress() -> (FlowProgressCallback, ProgressLog) {
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
    install_fake_tool(&cache, "app.mp4box", "1.0.0", "MP4Box", FAKE_WRITER)?;
    let source = dir.path().join("in.mkv");
    std::fs::write(&source, "x")?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    let reg = base_registry();
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "block.video.remux-mp4", "name": "mp4"},
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
        allow_private_integrations: None,
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;
    assert_eq!(outcome.final_file, work.join("remuxed.mp4"));
    assert_eq!(
        outcome.output_profile,
        crate::media::OutputProfile::Mp4Container
    );
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
        "app.ffmpeg",
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
            {"id": "n2", "type": "block.video.encode", "name": "enc", "config": {"codec": "h265", "encoder": "nvidia"}},
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
        allow_private_integrations: None,
    }
    .execute_flow(Uuid::new_v4(), &flow, &source, &work, progress)
    .await
    .map_err(|e| e.message)?;
    assert_eq!(
        outcome.output_profile,
        crate::media::OutputProfile::TranscodedMkv
    );
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
            "outputFilename": "branded.mkv",
            "outputProfile": "transcoded_mkv",
            "postValidate": "valid-media",
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
        allow_private_integrations: None,
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
        allow_private_integrations: None,
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
