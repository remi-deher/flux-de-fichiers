//! Routes liées aux variables de script (ADR 0025) : aperçu d'un script de dépôt, variables d'un
//! nœud du flux et refus d'une référence inconnue à l'enregistrement d'un flux.

mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forgearr::{
    api::{AppState, router},
    config::Config,
    plugins::save_local_script,
};
use forgearr_shared::{
    local::{ScriptInterpreter, UpsertLocalScriptRequest},
    script_variables::{ScriptVariable, VariableKind},
};
use serde_json::{Value, json};
use sqlx::sqlite::SqlitePoolOptions;
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_TEST_FILE_BYTES: u64 = 1024;
const WEB_SESSION_TTL_SECONDS: i64 = 28_800;
const REPOSITORY_ID: &str = "repo.test";
const SCRIPT_CODE: &str = "# note\necho ok\n";
const DEFAULT_CRF: i64 = 23;
const SAMPLE_WORKING_FILE: &str = "/travail/fichier.mkv";

/// Application de test : ses dossiers temporaires restent vivants tant que la valeur existe.
struct TestApp {
    router: axum::Router,
    config_dir: tempfile::TempDir,
    plugins_dir: tempfile::TempDir,
}

async fn test_app() -> anyhow::Result<TestApp> {
    let config_dir = tempfile::tempdir()?;
    let plugins_dir = tempfile::tempdir()?;
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: config_dir.path().to_path_buf(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: Some(plugins_dir.path().to_path_buf()),
        static_dir: "frontend/dist".into(),
        max_file_bytes: MAX_TEST_FILE_BYTES,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: WEB_SESSION_TTL_SECONDS,
        tls: None,
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    Ok(TestApp {
        router: router(AppState::new(pool, Arc::new(config))),
        config_dir,
        plugins_dir,
    })
}

fn variable(name: &str, kind: VariableKind, default: Option<Value>) -> ScriptVariable {
    ScriptVariable {
        name: name.into(),
        kind,
        default_value: default,
        ..ScriptVariable::default()
    }
}

/// Écrit le script « Encodeur » (variables `crf` et `cible`) dans `dir` et renvoie son identifiant.
fn save_encoder_script(dir: &Path) -> anyhow::Result<String> {
    let saved = save_local_script(
        dir,
        UpsertLocalScriptRequest {
            variables: vec![
                variable("crf", VariableKind::Number, Some(json!(DEFAULT_CRF))),
                variable("cible", VariableKind::Text, None),
            ],
            id: None,
            name: "Encodeur".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: SCRIPT_CODE.into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;
    Ok(saved.id)
}

fn authorized(method: &str, uri: &str, body: Option<&Value>) -> anyhow::Result<Request<Body>> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", common::ADMIN_KEY));
    Ok(match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(json)?))?,
        None => builder.body(Body::empty())?,
    })
}

async fn send(app: &TestApp, request: Request<Body>) -> anyhow::Result<(StatusCode, Value)> {
    let response = app.router.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), MAX_BODY_BYTES).await?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, body))
}

/// Graphe `in → enc → out` ; `enc` exécute le script « Encodeur » avec la config donnée.
fn encoder_graph(config: Value) -> Value {
    json!({
        "nodes": [
            {"id": "in", "type": "file-input", "name": "Entrée"},
            {"id": "enc", "type": "block.script.encodeur", "name": "Encodeur", "config": config},
            {"id": "out", "type": "file-output", "name": "Sortie"}
        ],
        "connections": [
            {"from": "in", "to": "enc"},
            {"from": "enc", "to": "out"}
        ]
    })
}

fn flow_payload(graph: &Value) -> Value {
    json!({
        "name": "Flux variables",
        "description": "Essai des variables de script",
        "file_mask": "*.mkv",
        "is_default": false,
        "definition": graph.to_string()
    })
}

#[tokio::test]
async fn repository_script_preview_returns_code_tokens_variables_and_requirements()
-> anyhow::Result<()> {
    let app = test_app().await?;
    let repository = app.config_dir.path().join("plugins").join(REPOSITORY_ID);
    let script_id = save_encoder_script(&repository)?;
    let operation = repository.join("scripts/encodeur/operations/run.json");
    let mut contract: Value = serde_json::from_str(&std::fs::read_to_string(&operation)?)?;
    contract["runtime"]["requires"] =
        json!([{"variable": "mediainfo.video.codec", "provider": "app.mediainfo"}]);
    std::fs::write(&operation, contract.to_string())?;

    let uri = format!("/api/v1/plugins/repositories/{REPOSITORY_ID}/scripts/{script_id}");
    let (status, body) = send(&app, authorized("GET", &uri, None)?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["script"]["code"], json!(SCRIPT_CODE));
    assert_eq!(body["script"]["variables"][0]["name"], json!("crf"));
    assert_eq!(
        body["script"]["variables"][0]["default"],
        json!(DEFAULT_CRF)
    );
    let first_line = body["lines"][0].as_array().cloned().unwrap_or_default();
    assert!(
        first_line
            .iter()
            .any(|token| token["kind"] == json!("comment"))
    );
    assert_eq!(body["requirements"][0]["available"], json!(false));
    assert_eq!(
        body["requirements"][0]["requirement"]["variable"],
        json!("mediainfo.video.codec")
    );
    assert_eq!(body["dependenciesUnverified"], json!(false));
    Ok(())
}

#[tokio::test]
async fn repository_script_preview_is_not_found_for_unknown_or_unsafe_targets() -> anyhow::Result<()>
{
    let app = test_app().await?;
    let repository = app.config_dir.path().join("plugins").join(REPOSITORY_ID);
    let script_id = save_encoder_script(&repository)?;

    for uri in [
        format!("/api/v1/plugins/repositories/{REPOSITORY_ID}/scripts/script.absent"),
        format!("/api/v1/plugins/repositories/repo.absent/scripts/{script_id}"),
        format!("/api/v1/plugins/repositories/bad%20id/scripts/{script_id}"),
    ] {
        let (status, _) = send(&app, authorized("GET", &uri, None)?).await?;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    Ok(())
}

#[tokio::test]
async fn the_new_variable_routes_require_authentication() -> anyhow::Result<()> {
    let app = test_app().await?;
    let preview = Request::builder()
        .uri(format!(
            "/api/v1/plugins/repositories/{REPOSITORY_ID}/scripts/script.x"
        ))
        .body(Body::empty())?;
    let (status, _) = send(&app, preview).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let node_variables = Request::builder()
        .method("POST")
        .uri("/api/v1/flows/node-variables")
        .header("content-type", "application/json")
        .body(Body::from(json!({"graph": {}, "nodeId": "x"}).to_string()))?;
    let (status, _) = send(&app, node_variables).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn node_variables_return_the_schema_references_and_the_resolved_config() -> anyhow::Result<()>
{
    let app = test_app().await?;
    save_encoder_script(app.plugins_dir.path())?;
    let request = |graph: Value, node_id: &str| json!({"graph": graph, "nodeId": node_id, "mediaInfo": {"video": {"codec": "hevc"}}});

    let graph = encoder_graph(json!({"cible": "{{workingFile}}"}));
    let (status, body) = send(
        &app,
        authorized(
            "POST",
            "/api/v1/flows/node-variables",
            Some(&request(graph, "enc")),
        )?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["variables"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["resolved"]["crf"], json!(DEFAULT_CRF));
    assert_eq!(body["resolved"]["cible"], json!(SAMPLE_WORKING_FILE));
    assert!(body.get("error").is_none());
    let references: Vec<&str> = body["references"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|r| r["reference"].as_str())
                .collect()
        })
        .unwrap_or_default();
    for expected in ["workingFile", "mediaInfo.video.codec", "nodes.in.message"] {
        assert!(
            references.contains(&expected),
            "{expected} absent de {references:?}"
        );
    }
    assert!(
        !references.contains(&"nodes.out.message"),
        "un nœud en aval n'est pas proposé"
    );
    Ok(())
}

#[tokio::test]
async fn node_variables_explain_a_failed_resolution_and_ignore_non_script_nodes()
-> anyhow::Result<()> {
    let app = test_app().await?;
    save_encoder_script(app.plugins_dir.path())?;
    let call = |config: Value, node_id: &'static str| {
        authorized(
            "POST",
            "/api/v1/flows/node-variables",
            Some(&json!({"graph": encoder_graph(config), "nodeId": node_id})),
        )
    };

    let (status, body) = send(&app, call(json!({"cible": "{{mediaInfo.absent}}"}), "enc")?).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("resolved").is_none());
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|message| message.contains("cible"))
    );

    let (_, unknown) = send(&app, call(json!({}), "inconnu")?).await?;
    assert!(unknown["error"].is_string());

    let (status, not_a_script) = send(&app, call(json!({}), "in")?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(not_a_script["variables"].as_array().map(Vec::len), Some(0));
    Ok(())
}

#[tokio::test]
async fn saving_a_flow_refuses_unknown_or_downstream_references_and_accepts_valid_ones()
-> anyhow::Result<()> {
    let app = test_app().await?;
    save_encoder_script(app.plugins_dir.path())?;

    for (reference, expected) in [
        ("{{inconnue.var}}", "inconnue.var"),
        ("{{nodes.out.message}}", "nodes.out.message"),
        ("{{workingFile.sous}}", "workingFile.sous"),
    ] {
        let graph = encoder_graph(json!({"cible": reference}));
        let (status, body) = send(
            &app,
            authorized("PUT", "/api/v1/flows/flow.var", Some(&flow_payload(&graph)))?,
        )
        .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{reference}");
        assert_eq!(body["error"]["code"], json!("invalid_flow_variables"));
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains(expected)),
            "{reference} : {body}"
        );
    }

    let valid = encoder_graph(json!({"cible": "{{workingFile}} {{nodes.in.message}}"}));
    let (status, _) = send(
        &app,
        authorized("PUT", "/api/v1/flows/flow.var", Some(&flow_payload(&valid)))?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn flow_validation_reports_invalid_variable_references_in_its_errors() -> anyhow::Result<()> {
    let app = test_app().await?;
    save_encoder_script(app.plugins_dir.path())?;

    let invalid = encoder_graph(json!({"cible": "{{inconnue.var}}"}));
    let (status, report) = send(
        &app,
        authorized(
            "POST",
            "/api/v1/flows/validate",
            Some(&json!({"definition": invalid})),
        )?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["valid"], json!(false));
    assert!(report["errors"].as_array().is_some_and(|errors| {
        errors.iter().any(|error| {
            error
                .as_str()
                .is_some_and(|text| text.contains("inconnue.var"))
        })
    }));

    // Le catalogue de ce test ne connaît pas les blocs d'entrée et de sortie : seule l'absence
    // d'erreur de référence est vérifiée.
    let valid = encoder_graph(json!({"cible": "{{workingFile}}"}));
    let (_, report) = send(
        &app,
        authorized(
            "POST",
            "/api/v1/flows/validate",
            Some(&json!({"definition": valid})),
        )?,
    )
    .await?;
    assert!(report["errors"].as_array().is_some_and(|errors| {
        errors.iter().all(|error| {
            error
                .as_str()
                .is_some_and(|text| !text.contains("référence"))
        })
    }));
    Ok(())
}

/// Écrit dans `dir` un plugin dont l'opération expose `namespace` ; renvoie son entrée d'index.
fn write_exposing_plugin(dir: &Path, plugin_id: &str, namespace: &str) -> anyhow::Result<Value> {
    let plugin_dir = dir.join("plugins").join(plugin_id);
    std::fs::create_dir_all(plugin_dir.join("operations"))?;
    std::fs::write(
        plugin_dir.join("manifest.json"),
        json!({
            "schemaVersion": "1.1.0", "id": plugin_id, "type": "plugin", "version": "1.0.0",
            "name": plugin_id,
            "files": [{"path": "operations/probe.json", "role": "operation"}]
        })
        .to_string(),
    )?;
    std::fs::write(
        plugin_dir.join("operations/probe.json"),
        json!({
            "schemaVersion": "1.1.0", "id": format!("operation.{plugin_id}.probe"), "version": "1.0.0",
            "name": "Sonde", "providerId": plugin_id, "providerKind": "plugin",
            "capability": "probe", "status": "validated",
            "runtime": {"kind": "primitive", "primitive": "entry"},
            "exposes": {
                "namespace": namespace, "contractVersion": "1.0.0",
                "variables": [{"name": "video.codec", "label": "Codec vidéo", "path": "video.codec"}]
            }
        })
        .to_string(),
    )?;
    Ok(json!({
        "id": plugin_id, "type": "plugin", "version": "1.0.0", "name": plugin_id,
        "manifestPath": format!("plugins/{plugin_id}/manifest.json")
    }))
}

fn write_catalogue(dir: &Path, entries: Vec<Value>) -> anyhow::Result<()> {
    std::fs::write(
        dir.join("index.json"),
        json!({"schemaVersion": "1.0.0", "entries": entries}).to_string(),
    )?;
    Ok(())
}

#[tokio::test]
async fn exposed_variables_list_every_exposure_with_the_state_of_its_namespace()
-> anyhow::Result<()> {
    let app = test_app().await?;
    let dir = app.plugins_dir.path();
    write_catalogue(
        dir,
        vec![
            write_exposing_plugin(dir, "alpha", "mediainfo")?,
            write_exposing_plugin(dir, "beta", "mediainfo")?,
            write_exposing_plugin(dir, "gamma", "autre")?,
        ],
    )?;

    let (status, body) = send(
        &app,
        authorized("GET", "/api/v1/plugins/exposed-variables", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let views = body.as_array().cloned().unwrap_or_default();
    assert_eq!(views.len(), 3);
    let view_of = |provider: &str| {
        views
            .iter()
            .find(|view| view["provider"] == json!(provider))
    };
    let alpha = view_of("alpha").ok_or_else(|| anyhow::anyhow!("alpha absent"))?;
    let beta = view_of("beta").ok_or_else(|| anyhow::anyhow!("beta absent"))?;
    let gamma = view_of("gamma").ok_or_else(|| anyhow::anyhow!("gamma absent"))?;
    assert_eq!(alpha["active"], json!(true));
    assert_eq!(alpha["conflictsWith"], json!(["beta"]));
    assert_eq!(beta["active"], json!(false));
    assert_eq!(beta["conflictsWith"], json!(["alpha"]));
    assert_eq!(gamma["conflictsWith"], json!([]));
    assert_eq!(alpha["exposes"]["namespace"], json!("mediainfo"));
    assert_eq!(
        alpha["exposes"]["variables"][0]["path"],
        json!("video.codec")
    );
    Ok(())
}

#[tokio::test]
async fn a_plugin_sheet_carries_its_own_exposures_and_a_plain_plugin_has_none() -> anyhow::Result<()>
{
    let app = test_app().await?;
    let dir = app.plugins_dir.path();
    let plain_dir = dir.join("plugins/simple");
    std::fs::create_dir_all(&plain_dir)?;
    std::fs::write(
        plain_dir.join("manifest.json"),
        json!({"schemaVersion": "1.1.0", "id": "simple", "type": "plugin", "version": "1.0.0", "name": "Simple"})
            .to_string(),
    )?;
    write_catalogue(
        dir,
        vec![
            write_exposing_plugin(dir, "alpha", "mediainfo")?,
            json!({"id": "simple", "type": "plugin", "version": "1.0.0", "name": "Simple",
                   "manifestPath": "plugins/simple/manifest.json"}),
        ],
    )?;

    let (status, alpha) = send(&app, authorized("GET", "/api/v1/plugins/alpha", None)?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(alpha["exposures"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        alpha["exposures"][0]["exposes"]["namespace"],
        json!("mediainfo")
    );
    assert_eq!(alpha["exposures"][0]["provider"], json!("alpha"));

    let (_, simple) = send(&app, authorized("GET", "/api/v1/plugins/simple", None)?).await?;
    assert!(simple.get("exposures").is_none(), "{simple}");

    let anonymous = Request::builder()
        .uri("/api/v1/plugins/exposed-variables")
        .body(Body::empty())?;
    assert_eq!(send(&app, anonymous).await?.0, StatusCode::UNAUTHORIZED);
    Ok(())
}
