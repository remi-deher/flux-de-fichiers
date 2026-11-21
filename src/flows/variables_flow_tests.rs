//! Tests du moteur pour les variables de flux : bloc « Définir une variable », conditions, relais.

use super::engine_tests::flow_record;
use super::placement_flow_tests::{placement_registry, run_engine};
use super::*;
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(super) fn variables_registry() -> OperationRegistry {
    let mut reg = placement_registry();
    for (id, primitive) in [
        ("operation.core.set-variable", "set-variable"),
        ("operation.core.condition", "condition"),
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
    reg.register_block_mapping("set-variable", "operation.core.set-variable");
    reg.register_block_mapping("condition", "operation.core.condition");
    reg
}

/// Entrée -> condition -> sortie `yes` (vrai) ou `no` (faux), après des blocs intermédiaires.
fn branch_flow(before: serde_json::Value, condition: serde_json::Value) -> FlowRecord {
    flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            {"id": "set", "type": "set-variable", "name": "set", "config": before},
            {"id": "cond", "type": "condition", "name": "cond", "config": condition},
            {"id": "yes", "type": "file-output", "name": "yes"},
            {"id": "no", "type": "file-output", "name": "no"}
        ],
        "connections": [
            {"from": "in", "to": "set"},
            {"from": "set", "to": "cond"},
            {"from": "cond", "to": "yes", "port": "true"},
            {"from": "cond", "to": "no", "port": "false"}
        ]
    }))
}

fn last_node(outcome: &FlowExecutionOutcome) -> Option<&str> {
    outcome.steps.last().map(|step| step.node_id.as_str())
}

async fn run(flow: &FlowRecord, run_info: FlowRunInfo<'_>) -> Result<FlowExecutionOutcome, String> {
    run_engine(&variables_registry(), flow, None, None, run_info).await
}

#[tokio::test]
async fn a_set_variable_drives_a_condition() -> TestResult {
    let flow = branch_flow(
        json!({"name": "codec", "value": "hevc"}),
        json!({"property": "vars.codec", "operator": "eq", "target": "hevc"}),
    );
    let outcome = run(&flow, FlowRunInfo::default()).await?;
    assert_eq!(last_node(&outcome), Some("yes"));
    assert_eq!(
        outcome.steps[1].message.as_deref(),
        Some("variable vars.codec définie")
    );
    Ok(())
}

#[tokio::test]
async fn values_and_targets_are_rendered_from_intrinsic_variables() -> TestResult {
    // `in.mkv` : la variable reçoit l'extension, puis la cible compare deux modèles.
    let flow = branch_flow(
        json!({"name": "ext", "value": "{$file.ext}"}),
        json!({"property": "vars.ext", "operator": "exact", "target": "{$file.ext}"}),
    );
    let outcome = run(&flow, FlowRunInfo::default()).await?;
    assert_eq!(last_node(&outcome), Some("yes"));

    let flow = branch_flow(
        json!({"name": "ext", "value": "x"}),
        json!({"property": "library.name", "operator": "eq", "target": "Films"}),
    );
    let info = FlowRunInfo {
        library_name: Some("Films"),
        ..FlowRunInfo::default()
    };
    assert_eq!(last_node(&run(&flow, info).await?), Some("yes"));
    assert_eq!(
        last_node(&run(&flow, FlowRunInfo::default()).await?),
        Some("no")
    );
    Ok(())
}

#[tokio::test]
async fn an_invalid_variable_name_fails_the_step() {
    let flow = branch_flow(
        json!({"name": "nom invalide", "value": "x"}),
        json!({"property": "vars.x", "operator": "exists"}),
    );
    let error = run(&flow, FlowRunInfo::default()).await;
    assert!(matches!(error, Err(message) if message.contains("nom invalide")));
}

#[tokio::test]
async fn a_relay_carries_custom_variables_and_the_resume_restores_them() -> TestResult {
    let flow = flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            {"id": "set", "type": "set-variable", "name": "set", "config": {"name": "codec", "value": "hevc"}},
            {"id": "relay", "type": "relay", "name": "relay", "config": {"mode": "capable", "codec": "hevc"}},
            {"id": "cond", "type": "condition", "name": "cond", "config": {"property": "vars.codec", "target": "hevc"}},
            {"id": "yes", "type": "file-output", "name": "yes"},
            {"id": "no", "type": "file-output", "name": "no"}
        ],
        "connections": [
            {"from": "in", "to": "set"},
            {"from": "set", "to": "relay"},
            {"from": "relay", "to": "cond"},
            {"from": "cond", "to": "yes", "port": "true"},
            {"from": "cond", "to": "no", "port": "false"}
        ]
    }));
    let registry = variables_registry();
    let outcome = run_engine(&registry, &flow, None, None, FlowRunInfo::default()).await?;
    let request = outcome.relay.ok_or("un relais est attendu")?;
    assert_eq!(request.variables.get("codec"), Some(&json!("hevc")));

    let resumed = FlowRunInfo {
        resume_variables: Some(&request.variables),
        ..FlowRunInfo::default()
    };
    let outcome = run_engine(&registry, &flow, None, Some("cond"), resumed).await?;
    assert_eq!(last_node(&outcome), Some("yes"));
    let lost = run_engine(&registry, &flow, None, Some("cond"), FlowRunInfo::default()).await?;
    assert_eq!(last_node(&lost), Some("no"));
    Ok(())
}

#[tokio::test]
async fn invalid_resume_variables_fail_clearly() {
    let flow = branch_flow(json!({"name": "a", "value": "b"}), json!({}));
    let invalid = Map::from_iter([("nom invalide".to_owned(), json!(1))]);
    let info = FlowRunInfo {
        resume_variables: Some(&invalid),
        ..FlowRunInfo::default()
    };
    let error = run(&flow, info).await;
    assert!(matches!(error, Err(message) if message.contains("variables de reprise")));
}
