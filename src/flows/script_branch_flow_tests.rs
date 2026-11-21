//! Tests du moteur pour le routage par défaut du bloc Script conditionnel.

use super::engine_tests::{flow_record, register_scripts_run};
use super::placement_flow_tests::{placement_registry, run_engine};
use super::*;
use crate::domain::JobStepStatus;
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Entrée -> script (config donnée) -> `n_true` / `n_false`. L'arête `false` est stockée en
/// premier pour prouver que le moteur ne suit plus la première liaison venue.
fn branch_flow(script_config: serde_json::Value) -> FlowRecord {
    flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "scripts.run", "name": "Script", "config": script_config},
            {"id": "n_true", "type": "file-output", "name": "vrai"},
            {"id": "n_false", "type": "file-output", "name": "faux"}
        ],
        "connections": [
            {"from": "n1", "to": "n2"},
            {"from": "n2", "to": "n_false", "port": "false"},
            {"from": "n2", "to": "n_true", "port": "true"}
        ]
    }))
}

fn conditional_inline(code: &str) -> serde_json::Value {
    json!({"routing": "conditional", "mode": "inline", "interpreter": "sh", "code": code})
}

async fn run_branch(script_config: serde_json::Value) -> Result<FlowExecutionOutcome, String> {
    let mut reg = placement_registry();
    register_scripts_run(&mut reg).map_err(|e| e.to_string())?;
    run_engine(&reg, &branch_flow(script_config), None, None, FlowRunInfo::default()).await
}

fn visited(outcome: &FlowExecutionOutcome) -> Vec<&str> {
    outcome.steps.iter().map(|step| step.node_id.as_str()).collect()
}

#[tokio::test]
async fn conditional_script_without_next_port_routes_to_true() -> TestResult {
    let outcome =
        run_branch(conditional_inline("cat >/dev/null\nprintf '{\"status\":\"ok\"}'")).await?;
    assert_eq!(visited(&outcome), ["n1", "n2", "n_true"]);
    Ok(())
}

#[tokio::test]
async fn conditional_script_without_json_routes_to_true() -> TestResult {
    let outcome = run_branch(conditional_inline("cat >/dev/null")).await?;
    assert_eq!(visited(&outcome), ["n1", "n2", "n_true"]);
    Ok(())
}

#[tokio::test]
async fn conditional_script_skip_routes_to_false() -> TestResult {
    let outcome =
        run_branch(conditional_inline("cat >/dev/null\nprintf '{\"status\":\"skip\"}'")).await?;
    assert_eq!(visited(&outcome), ["n1", "n2", "n_false"]);
    assert_eq!(outcome.steps[1].status, JobStepStatus::Skipped);
    Ok(())
}

#[tokio::test]
async fn conditional_script_skip_honours_explicit_true() -> TestResult {
    let outcome = run_branch(conditional_inline(
        "cat >/dev/null\nprintf '{\"status\":\"skip\",\"nextPort\":\"true\"}'",
    ))
    .await?;
    assert_eq!(visited(&outcome), ["n1", "n2", "n_true"]);
    Ok(())
}

#[tokio::test]
async fn conditional_script_invalid_next_port_fails() {
    let result = run_branch(conditional_inline(
        "cat >/dev/null\nprintf '{\"status\":\"ok\",\"nextPort\":\"alt\"}'",
    ))
    .await;
    let message = result.err().unwrap_or_default();
    assert!(message.contains("alt"), "message inattendu : {message}");
}

#[tokio::test]
async fn single_script_without_port_follows_plain_edge() -> TestResult {
    let mut reg = placement_registry();
    register_scripts_run(&mut reg)?;
    let flow = flow_record(json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "in"},
            {"id": "n2", "type": "scripts.run", "name": "Script",
             "config": {"mode": "inline", "interpreter": "sh", "code": "cat >/dev/null\nprintf '{\"status\":\"skip\"}'"}},
            {"id": "n3", "type": "file-output", "name": "out"}
        ],
        "connections": [
            {"from": "n1", "to": "n2"},
            {"from": "n2", "to": "n3"}
        ]
    }));
    let outcome = run_engine(&reg, &flow, None, None, FlowRunInfo::default()).await?;
    assert_eq!(visited(&outcome), ["n1", "n2", "n3"]);
    Ok(())
}
