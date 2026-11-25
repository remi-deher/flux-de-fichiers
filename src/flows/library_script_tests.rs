//! `scriptId` d'un script installé, qui garde l'opération de son dépôt (ADR 0029).

use super::{OperationRegistry, OperationRuntime, engine_tests::flow_record};
use crate::plugins::{repair_local_index, save_local_script};
use forgearr_shared::{
    flows::{FlowGraph, FlowRecord},
    local::{ScriptInterpreter, UpsertLocalScriptRequest},
};
use serde_json::{Value, json};
use std::path::Path;

const CODE: &str = "echo route";
const ENTRY: &str = "route";
const OPERATION: &str = "operation.script.route";
/// Nom sous lequel un worker cherche le script.
const LIBRARY_OPERATION: &str = "operation.route";

/// Script d'identifiant `route` et d'opération `operation.script.route`, qui lance l'outil `interpreteurs`.
fn write_installed_script(local_dir: &Path) -> anyhow::Result<()> {
    save_local_script(
        local_dir,
        UpsertLocalScriptRequest {
            name: "Route".into(),
            interpreter: ScriptInterpreter::Sh,
            code: CODE.into(),
            ..UpsertLocalScriptRequest::default()
        },
    )?;
    let edit = |file: &str, change: fn(&mut Value)| -> anyhow::Result<()> {
        let path = local_dir.join("scripts/route").join(file);
        let mut document: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        change(&mut document);
        std::fs::write(path, document.to_string())?;
        Ok(())
    };
    edit("manifest.json", |manifest| manifest["id"] = json!(ENTRY))?;
    edit("operations/run.json", |operation| {
        operation["runtime"]["tool"] = json!({"id": "interpreteurs", "version": "1.1.0"});
        operation["runtime"]["acceptsNonMedia"] = json!(true);
    })?;
    std::fs::remove_file(local_dir.join("index.json"))?;
    anyhow::ensure!(repair_local_index(local_dir)?);
    Ok(())
}

fn flow(script_id: &str) -> FlowRecord {
    flow_record(json!({
        "nodes": [{
            "id": "n", "type": "scripts.run", "name": "N",
            "config": {"mode": "library", "scriptId": script_id}
        }],
        "connections": []
    }))
}

#[test]
fn an_installed_script_is_found_by_its_identifier_and_by_a_worker() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    write_installed_script(local.path())?;
    let registry = OperationRegistry::load_from_roots(&[local.path().to_path_buf()]);
    let flow = flow(ENTRY);
    let graph = FlowGraph::parse(&flow.definition);
    let found = registry.library_script_operation(&graph.nodes[0]);
    assert_eq!(
        found.map(|operation| operation.id.as_str()),
        Some(OPERATION)
    );
    let tools = registry.required_tool_refs_for_flow(Some(&flow));
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].id, "interpreteurs");
    assert!(registry.graph_accepts_non_media(&graph));

    let (operations, blocks) = registry.claim_payload_for_flow(Some(&flow));
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[LIBRARY_OPERATION]["id"], LIBRARY_OPERATION);
    let received = OperationRegistry::from_raw(operations, blocks);
    let Some(operation) = received.library_script_operation(&graph.nodes[0]) else {
        anyhow::bail!("script absent du bail");
    };
    let OperationRuntime::Script(script) = &operation.runtime else {
        anyhow::bail!("{:?}", operation.runtime);
    };
    assert_eq!(script.code.as_deref(), Some(CODE));
    Ok(())
}

#[test]
fn a_user_script_keeps_its_own_operation_and_an_unknown_one_has_none() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    let saved = save_local_script(
        local.path(),
        UpsertLocalScriptRequest {
            name: "Perso".into(),
            code: CODE.into(),
            ..UpsertLocalScriptRequest::default()
        },
    )?;
    let registry = OperationRegistry::load_from_roots(&[local.path().to_path_buf()]);
    let (operations, _) = registry.claim_payload_for_flow(Some(&flow(&saved.id)));
    assert_eq!(
        operations.keys().collect::<Vec<_>>(),
        ["operation.script.perso"]
    );
    let (operations, _) = registry.claim_payload_for_flow(Some(&flow("absent")));
    assert!(operations.is_empty());
    Ok(())
}
