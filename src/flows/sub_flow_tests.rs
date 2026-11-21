//! Tests du moteur pour le bloc « Aller au sous-flux » et les variables du flux d'échec.

use super::engine_tests::flow_record;
use super::placement_flow_tests::run_engine;
use super::variables_flow_tests::variables_registry;
use super::*;
use forgearr_shared::flows::{LinkedFlowRole, MAX_SUB_FLOW_DEPTH};
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SUB_FLOW_OPERATION: &str = "operation.core.sub-flow";

fn sub_flow_registry() -> OperationRegistry {
    let mut reg = variables_registry();
    let registered = reg.register_json(json!({
        "schemaVersion": "1.1.0",
        "id": SUB_FLOW_OPERATION,
        "version": "1.0.0",
        "name": "Sous-flux",
        "providerId": "essentiel",
        "providerKind": "plugin",
        "capability": "control",
        "status": "validated",
        "runtime": { "kind": "primitive", "primitive": "sub-flow" }
    }));
    assert!(registered.is_ok(), "contrat de test valide : {registered:?}");
    reg.register_block_mapping("sub-flow", SUB_FLOW_OPERATION);
    reg
}

fn last_node(outcome: &FlowExecutionOutcome) -> Option<&str> {
    outcome.steps.last().map(|step| step.node_id.as_str())
}

fn named(id: &str, definition: serde_json::Value) -> FlowRecord {
    let mut record = flow_record(definition);
    record.id = id.to_owned();
    record
}

fn linked(flows: Vec<FlowRecord>) -> Vec<LinkedFlow> {
    flows
        .into_iter()
        .map(|flow| LinkedFlow {
            role: LinkedFlowRole::SubFlow,
            flow,
        })
        .collect()
}

/// Entrée -> appel de `target` -> condition `vars.codec == hevc` -> `yes` / `no`.
fn caller(target: &str) -> FlowRecord {
    flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            {"id": "call", "type": "sub-flow", "name": "call", "config": {"flowId": target}},
            {"id": "cond", "type": "condition", "name": "cond",
             "config": {"property": "vars.codec", "operator": "eq", "target": "hevc"}},
            {"id": "yes", "type": "file-output", "name": "yes"},
            {"id": "no", "type": "file-output", "name": "no"}
        ],
        "connections": [
            {"from": "in", "to": "call"},
            {"from": "call", "to": "cond"},
            {"from": "cond", "to": "yes", "port": "true"},
            {"from": "cond", "to": "no", "port": "false"}
        ]
    }))
}

/// Sous-flux qui écrit `vars.codec` puis se termine par une sortie.
fn setter(id: &str) -> FlowRecord {
    named(
        id,
        json!({
            "nodes": [
                {"id": "in", "type": "file-input", "name": "in"},
                {"id": "set", "type": "set-variable", "name": "set",
                 "config": {"name": "codec", "value": "hevc"}},
                {"id": "out", "type": "file-output", "name": "out"}
            ],
            "connections": [{"from": "in", "to": "set"}, {"from": "set", "to": "out"}]
        }),
    )
}

/// Flux dont l'unique traitement est l'appel de `target`.
fn forwarder(id: &str, target: &str) -> FlowRecord {
    named(
        id,
        json!({
            "nodes": [
                {"id": "in", "type": "file-input", "name": "in"},
                {"id": "call", "type": "sub-flow", "name": "call", "config": {"flowId": target}}
            ],
            "connections": [{"from": "in", "to": "call"}]
        }),
    )
}

async fn run_with(
    flow: &FlowRecord,
    links: &[LinkedFlow],
) -> Result<FlowExecutionOutcome, String> {
    let info = FlowRunInfo {
        linked_flows: links,
        ..FlowRunInfo::default()
    };
    run_engine(&sub_flow_registry(), flow, None, None, info).await
}

#[tokio::test]
async fn a_sub_flow_runs_then_returns_to_the_caller_output() -> TestResult {
    let sub = linked(vec![setter("flow.sub")]);
    let outcome = run_with(&caller("flow.sub"), &sub).await?;
    let nodes: Vec<&str> = outcome.steps.iter().map(|s| s.node_id.as_str()).collect();
    assert_eq!(nodes, vec!["in", "call", "in", "set", "out", "cond", "yes"]);
    assert_eq!(
        outcome.steps[1].message.as_deref(),
        Some("sous-flux « flow.sub » (v1) exécuté")
    );
    Ok(())
}

#[tokio::test]
async fn a_sub_flow_that_was_not_frozen_fails_the_step() {
    let error = run_with(&caller("flow.absent"), &[]).await;
    assert!(matches!(error, Err(message) if message.contains("n'a pas été figé")));
}

#[tokio::test]
async fn a_recursive_call_is_refused_at_run_time() {
    // `flow` appelle `flow.sub`, qui rappelle `flow` : refusé même si les deux sont figés.
    let sub = linked(vec![forwarder("flow.sub", "flow"), forwarder("flow", "flow.sub")]);
    let error = run_with(&caller("flow.sub"), &sub).await;
    assert!(matches!(error, Err(message) if message.contains("récursif")));
}

#[tokio::test]
async fn nesting_beyond_the_maximum_depth_is_refused() {
    let mut flows: Vec<FlowRecord> = (0..MAX_SUB_FLOW_DEPTH)
        .map(|level| forwarder(&format!("f{level}"), &format!("f{}", level + 1)))
        .collect();
    flows.push(setter(&format!("f{MAX_SUB_FLOW_DEPTH}")));
    let error = run_with(&caller("f0"), &linked(flows)).await;
    assert!(matches!(error, Err(message) if message.contains("profondeur maximale")));
}

#[tokio::test]
async fn nesting_up_to_the_maximum_depth_is_accepted() -> TestResult {
    let deepest = MAX_SUB_FLOW_DEPTH - 1;
    let mut flows: Vec<FlowRecord> = (0..deepest)
        .map(|level| forwarder(&format!("f{level}"), &format!("f{}", level + 1)))
        .collect();
    flows.push(setter(&format!("f{deepest}")));
    let outcome = run_with(&caller("f0"), &linked(flows)).await?;
    assert_eq!(last_node(&outcome), Some("yes"));
    Ok(())
}

#[tokio::test]
async fn a_relay_inside_a_sub_flow_is_refused() {
    let relaying = named(
        "flow.relay",
        json!({
            "nodes": [
                {"id": "in", "type": "file-input", "name": "in"},
                {"id": "relay", "type": "relay", "name": "relay",
                 "config": {"mode": "capable", "codec": "hevc"}},
                {"id": "out", "type": "file-output", "name": "out"}
            ],
            "connections": [{"from": "in", "to": "relay"}, {"from": "relay", "to": "out"}]
        }),
    );
    let error = run_with(&caller("flow.relay"), &linked(vec![relaying])).await;
    assert!(matches!(error, Err(message) if message.contains("impossible dans un sous-flux")));
}

#[tokio::test]
async fn the_failure_flow_reads_the_error_variables() -> TestResult {
    let flow = flow_record(json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "in"},
            {"id": "cond", "type": "condition", "name": "cond",
             "config": {"property": "error.code", "operator": "exact", "target": "operation_failed"}},
            {"id": "set", "type": "set-variable", "name": "set",
             "config": {"name": "motif", "value": "{$error.message}"}},
            {"id": "no", "type": "file-output", "name": "no"}
        ],
        "connections": [
            {"from": "in", "to": "cond"},
            {"from": "cond", "to": "set", "port": "true"},
            {"from": "cond", "to": "no", "port": "false"}
        ]
    }));
    let info = FlowRunInfo {
        failure: Some(FailureInfo {
            code: "operation_failed",
            message: "encodage interrompu",
        }),
        ..FlowRunInfo::default()
    };
    let outcome = run_engine(&sub_flow_registry(), &flow, None, None, info).await?;
    assert_eq!(last_node(&outcome), Some("set"));

    let registry = sub_flow_registry();
    let without = run_engine(&registry, &flow, None, None, FlowRunInfo::default()).await?;
    assert_eq!(last_node(&without), Some("no"));
    Ok(())
}
