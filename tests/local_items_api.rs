//! Collection locale et éléments installés (ADR 0029) : édition et suppression d'un script, ressource figée, blocs
//! inconnus d'un flux.

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use common::store::{TestRepository, entry_runtime, install};
use forgearr::{
    api::{AppState, router},
    plugins::{self, ScriptCopy},
    repository,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use tower::ServiceExt;

const BODY_LIMIT_BYTES: usize = 1024 * 1024;
const SCRIPT: &str = "route";
const SCRIPT_URI: &str = "/api/v1/local/items/route";
const FLOW_URI: &str = "/api/v1/flows/flow.a";

struct Central {
    state: AppState,
    repository: PathBuf,
    _dir: tempfile::TempDir,
}

impl Central {
    /// `entree`, le script `route` et l'image `logo` sont installés ; `autre` est publié, pas installé.
    async fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let state = installation::blank_state(&dir.path().join("config")).await?;
        common::install_keys(&state.pool).await?;
        common::mark_store_migrated(&state.pool).await?;
        let repository = TestRepository::new(dir.path().join("depot"))
            .plugin("entree", &entry_runtime())?
            .plugin("autre", &entry_runtime())?
            .script(SCRIPT, "print('route')")?
            .image("logo", "logo.svg", "<svg/>")?
            .publish()?;
        for item in ["entree", SCRIPT, "logo"] {
            install(&state, &repository, item).await?;
        }
        Ok(Self {
            state,
            repository,
            _dir: dir,
        })
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let (status, bytes) = self.raw(method, uri, body).await?;
        Ok((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }

    async fn raw(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> anyhow::Result<(StatusCode, Vec<u8>)> {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |json| Body::from(json.to_string())))?;
        let response = router(self.state.clone()).oneshot(request).await?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), BODY_LIMIT_BYTES).await?;
        Ok((status, bytes.to_vec()))
    }

    async fn save_flow(&self, kinds: &[&str]) -> anyhow::Result<(StatusCode, Value)> {
        self.call("PUT", FLOW_URI, Some(flow(kinds))).await
    }

    fn local_dir(&self) -> PathBuf {
        plugins::local_catalog_dir(&self.state.config)
    }

    async fn is_installed(&self, item: &str) -> anyhow::Result<bool> {
        let mut conn = self.state.pool.acquire().await?;
        Ok(repository::get_installation(&mut conn, item)
            .await?
            .is_some())
    }

    async fn script_copy(&self) -> anyhow::Result<ScriptCopy> {
        let installations = repository::list_installations(&self.state.pool).await?;
        let frozen = installations
            .iter()
            .find(|item| item.record.item_id == SCRIPT)
            .and_then(|item| {
                plugins::installed_root(&self.state.config.installed_dir(), &item.record)
            })
            .ok_or_else(|| anyhow::anyhow!("script non inscrit"))?;
        Ok(plugins::compare_script_copy(
            &frozen,
            &self.local_dir(),
            SCRIPT,
        )?)
    }
}

fn definition(kinds: &[&str]) -> Value {
    let nodes: Vec<Value> = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| json!({"id": format!("n{index}"), "type": kind, "name": kind}))
        .collect();
    json!({"nodes": nodes, "connections": []})
}

fn flow(kinds: &[&str]) -> Value {
    json!({
        "name": "Mes films", "description": null, "file_mask": "*", "is_default": false,
        "definition": definition(kinds).to_string()
    })
}

#[tokio::test]
async fn an_installed_script_is_edited_under_its_own_identifiers() -> anyhow::Result<()> {
    let central = Central::new().await?;
    assert_eq!(central.script_copy().await?, ScriptCopy::Original);
    let (_, library) = central.call("GET", "/api/v1/local", None).await?;
    let script = &library["scripts"][0];
    assert_eq!(script["id"], SCRIPT);

    let edit = |code: &str| {
        json!({
            "id": SCRIPT, "name": script["name"], "interpreter": script["interpreter"],
            "filename": script["entrypoint"], "code": code
        })
    };
    for (code, expected) in [
        ("print('route')", ScriptCopy::Original),
        ("print('autre')", ScriptCopy::Modified),
    ] {
        let (status, saved) = central
            .call("POST", "/api/v1/local/scripts", Some(edit(code)))
            .await?;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(saved["id"], SCRIPT);
        assert_eq!(central.script_copy().await?, expected, "{code}");
    }
    // Son bloc reste celui que citent les flux.
    let (status, body) = central.save_flow(&["entree.run", "route-run"]).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_used_script_is_not_deleted_and_the_refusal_names_its_use() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let (status, _) = central.save_flow(&["entree.run", "route.run"]).await?;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = central.call("DELETE", SCRIPT_URI, None).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "item_in_use");
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("Mes films"), "{message}");
    assert!(central.is_installed(SCRIPT).await?);
    assert_eq!(central.script_copy().await?, ScriptCopy::Original);

    let (status, _) = central.save_flow(&["entree.run"]).await?;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = central.call("DELETE", SCRIPT_URI, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(!central.is_installed(SCRIPT).await?);
    let (_, library) = central.call("GET", "/api/v1/local", None).await?;
    assert_eq!(library["scripts"], json!([]));
    let (status, _) = central.call("DELETE", SCRIPT_URI, None).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Seule la collection se supprime par ici.
    let (status, _) = central
        .call("DELETE", "/api/v1/local/items/entree", None)
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(central.is_installed("entree").await?);
    Ok(())
}

#[tokio::test]
async fn a_user_script_chosen_by_a_flow_is_not_deleted_either() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let script = json!({"name": "Perso", "interpreter": "sh", "code": "echo ok"});
    let (status, saved) = central
        .call("POST", "/api/v1/local/scripts", Some(script))
        .await?;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let mut graph = definition(&["entree.run"]);
    graph["nodes"][0]["config"] = json!({"scriptId": "script.perso"});
    let mut payload = flow(&[]);
    payload["definition"] = json!(graph.to_string());
    let (status, _) = central.call("PUT", FLOW_URI, Some(payload)).await?;
    assert_eq!(status, StatusCode::OK);

    let uri = "/api/v1/local/items/script.perso";
    let (status, _) = central.call("DELETE", uri, None).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = central.call("DELETE", FLOW_URI, None).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = central.call("DELETE", uri, None).await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn the_file_of_an_installed_resource_is_downloaded_from_its_frozen_copy() -> anyhow::Result<()>
{
    let central = Central::new().await?;
    std::fs::remove_dir_all(&central.repository)?;
    let uri = "/api/v1/local/items/logo/file";
    let (status, content) = central.raw("GET", uri, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content, b"<svg/>");
    let (status, _) = central
        .call("GET", "/api/v1/local/items/absent/file", None)
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn a_flow_only_gains_blocks_that_are_installed() -> anyhow::Result<()> {
    let central = Central::new().await?;
    // Publié par un dépôt, pas installé.
    let (status, body) = central.save_flow(&["entree.run", "autre.run"]).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "unknown_blocks");
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("autre.run") && message.contains("depuis le Store"));
    let validate = json!({"definition": definition(&["entree.run", "autre.run"])});
    let (_, report) = central
        .call("POST", "/api/v1/flows/validate", Some(validate))
        .await?;
    assert_eq!(report["valid"], false);
    let errors = report["errors"].to_string();
    assert!(errors.contains("depuis le Store"), "{errors}");

    // Un flux d'avant, dont un bloc a disparu, reste modifiable et restaurable.
    for kinds in [["autre.run"], ["entree.run"], ["autre.run"]] {
        let request = serde_json::from_value(flow(&kinds))?;
        repository::save_flow(&central.state.pool, "flow.a", &request).await?;
    }
    let kept = ["entree.run", "autre.run", "autre-inconnu.run"];
    let (status, _) = central.save_flow(&kept[..2]).await?;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = central.save_flow(&kept).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let restore = |version: i64| format!("{FLOW_URI}/versions/{version}/restore");
    let (status, _) = central.call("POST", &restore(1), None).await?;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = central.call("POST", &restore(2), None).await?;
    assert_eq!(status, StatusCode::OK);
    // Le bloc n'est plus dans le flux : le remettre exige de l'installer.
    let (status, body) = central.call("POST", &restore(1), None).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    install(&central.state, &central.repository, "autre").await?;
    let (status, _) = central.call("POST", &restore(1), None).await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}
