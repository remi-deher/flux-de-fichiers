//! Dépôts de plugins écrits sur disque, comme après une synchronisation.

use forgearr::{
    api::AppState,
    plugins::{self, CatalogItemType},
    repository::{self, InstallationRecord},
};
use forgearr_shared::plugins::store::ItemOrigin;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const OFFICIAL: &str = "repo.official";
pub const TOOL_VERSION: &str = "1.2.0";

/// Une sonde qui traite aussi les tâches sans flux.
pub fn base_runtime() -> Value {
    json!({
        "kind": "binary", "executable": "mkvmerge", "tool": {"id": "mkvtoolnix"},
        "executionMode": "probe-primary", "defaultFallback": true
    })
}

pub fn tool_runtime(tool: &str) -> Value {
    json!({
        "kind": "binary", "executable": tool, "tool": {"id": tool}, "executionMode": "staged-output"
    })
}

pub fn script_runtime() -> Value {
    json!({"kind": "script", "interpreter": "python3", "entrypoint": "run.py"})
}

pub fn entry_runtime() -> Value {
    json!({"kind": "primitive", "primitive": "entry"})
}

fn write(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Catalogue dont un seul plugin porte ces blocs, sans opération : un flux peut les citer.
pub fn write_blocks(root: &Path, kinds: &[&str]) -> anyhow::Result<()> {
    const OWNER: &str = "blocs";
    const MANIFEST: &str = "plugins/blocs/manifest.json";
    let mut entries = vec![json!({
        "id": OWNER, "type": "plugin", "version": "1.0.0", "name": OWNER, "manifestPath": MANIFEST
    })];
    write(
        &root.join(MANIFEST),
        &json!({
            "schemaVersion": "1.1.0", "id": OWNER, "type": "plugin", "version": "1.0.0",
            "name": OWNER
        })
        .to_string(),
    )?;
    for kind in kinds {
        let path = format!("plugins/blocs/blocks/{kind}.json");
        write(
            &root.join(&path),
            &json!({"schemaVersion": "1.1.0", "id": kind, "version": "1.0.0", "name": kind})
                .to_string(),
        )?;
        entries.push(json!({
            "id": kind, "type": "block", "owner": OWNER, "version": "1.0.0", "name": kind,
            "manifestPath": path
        }));
    }
    write(
        &root.join("index.json"),
        &json!({"schemaVersion": "1.0.0", "entries": entries}).to_string(),
    )?;
    Ok(())
}

/// Installe un élément de ce dépôt, comme le ferait le Store ; un script reçoit sa copie modifiable.
pub async fn install(state: &AppState, root: &Path, item_id: &str) -> anyhow::Result<()> {
    let origin = ItemOrigin {
        repository_id: OFFICIAL.to_owned(),
        repository_url: "https://officiel.example/index.json".to_owned(),
        repository_name: OFFICIAL.to_owned(),
    };
    let content = plugins::read_item(root, item_id)?;
    let is_script = content.entry.entry_type == CatalogItemType::Script;
    let record = InstallationRecord {
        item_id: item_id.to_owned(),
        kind: content.entry.entry_type,
        version: content.entry.version.clone(),
        content_sha256: content.content_sha256.clone(),
        origin: origin.clone(),
        copy_id: is_script.then(|| item_id.to_owned()),
        copy_pending: is_script,
        serves_every_task: false,
    };
    {
        let lock = plugins::lock_installations().await;
        plugins::freeze_item(&lock, &state.config.installed_dir(), content, &origin).await?;
        let mut conn = state.pool.acquire().await?;
        repository::save_installation(&mut conn, &record).await?;
    }
    // Écrit les copies modifiables en attente.
    plugins::resume_store(&state.pool, &state.config, &state.catalog_cache, None).await?;
    state.catalog_cache.invalidate().await;
    Ok(())
}

/// Dépôt en préparation ; `publish` écrit son index.
pub struct TestRepository {
    root: PathBuf,
    entries: Vec<Value>,
}

impl TestRepository {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            entries: Vec::new(),
        }
    }

    /// Le socle, trois plugins (dont un inutile), un script et leurs outils.
    pub fn official(root: PathBuf) -> anyhow::Result<Self> {
        Self::new(root)
            .plugin("essentiel", &entry_runtime())?
            .plugin("video", &base_runtime())?
            .plugin("audio", &tool_runtime("ffmpeg"))?
            .plugin("notif", &tool_runtime("curl"))?
            .plugin("inutile", &tool_runtime("inutile-outil"))?
            .script("route", "print('route')")?
            .tool("mkvtoolnix")?
            .tool("ffmpeg")?
            .tool("curl")?
            .tool("inutile-outil")
    }

    fn entry(&mut self, id: &str, kind: &str, owner: Option<&str>, manifest: &str) {
        let mut entry = json!({
            "id": id, "type": kind, "version": "1.0.0", "name": id, "manifestPath": manifest
        });
        if let Some(owner) = owner {
            entry["owner"] = json!(owner);
        }
        self.entries.push(entry);
    }

    /// Élément d'une opération et d'un bloc `<id>.run` (alias `<id>-run`).
    fn with_operation(
        mut self,
        (kind, folder): (&str, &str),
        id: &str,
        operation_id: &str,
        runtime: &Value,
    ) -> anyhow::Result<Self> {
        let base = format!("{folder}/{id}");
        let file = |name: &str| self.root.join(&base).join(name);
        write(
            &file("manifest.json"),
            &json!({
                "schemaVersion": "1.1.0", "id": id, "type": kind, "version": "1.0.0", "name": id,
                "files": [
                    {"path": "operations/run.json", "role": "operation"},
                    {"path": "blocks/run.json", "role": "block"},
                    {"path": "run.py", "role": "script"}
                ]
            })
            .to_string(),
        )?;
        write(
            &file("operations/run.json"),
            &json!({
                "schemaVersion": "1.1.0", "id": operation_id, "version": "1.0.0", "name": id,
                "providerId": id, "providerKind": kind, "capability": "custom",
                "status": "validated", "runtime": runtime
            })
            .to_string(),
        )?;
        write(
            &file("blocks/run.json"),
            &json!({
                "schemaVersion": "1.1.0", "id": format!("{id}.run"), "version": "1.0.0", "name": id,
                "aliases": [format!("{id}-run")], "operationId": operation_id
            })
            .to_string(),
        )?;
        self.entry(id, kind, None, &format!("{base}/manifest.json"));
        let block = format!("{base}/blocks/run.json");
        self.entry(&format!("{id}.run"), "block", Some(id), &block);
        Ok(self)
    }

    /// Plugin d'opération `operation.<id>.run`.
    pub fn plugin(self, id: &str, runtime: &Value) -> anyhow::Result<Self> {
        let operation = format!("operation.{id}.run");
        self.with_operation(("plugin", "plugins"), id, &operation, runtime)
    }

    /// Script d'opération `operation.script.<id>`.
    pub fn script(self, id: &str, code: &str) -> anyhow::Result<Self> {
        let operation = format!("operation.script.{id}");
        let written =
            self.with_operation(("script", "scripts"), id, &operation, &script_runtime())?;
        write(&written.root.join(format!("scripts/{id}/run.py")), code)?;
        Ok(written)
    }

    pub fn tool(mut self, id: &str) -> anyhow::Result<Self> {
        let folder = self.root.join("apps").join(id);
        write(
            &folder.join("manifest.json"),
            &json!({
                "schemaVersion": "1.1.0", "id": id, "type": "app", "version": "1.0.0", "name": id
            })
            .to_string(),
        )?;
        write(
            &folder.join("distribution.v1.json"),
            &json!({"toolId": id, "toolVersion": TOOL_VERSION, "artifacts": []}).to_string(),
        )?;
        self.entry(id, "app", None, &format!("apps/{id}/manifest.json"));
        Ok(self)
    }

    /// Image d'un seul fichier, dans un sous-dossier.
    pub fn image(mut self, id: &str, filename: &str, content: &str) -> anyhow::Result<Self> {
        let folder = self.root.join("resources").join(id);
        write(
            &folder.join("manifest.json"),
            &json!({
                "schemaVersion": "1.1.0", "id": id, "type": "resource.image", "version": "1.0.0",
                "name": id, "files": [{"path": format!("files/{filename}"), "role": "asset"}]
            })
            .to_string(),
        )?;
        write(&folder.join("files").join(filename), content)?;
        let manifest = format!("resources/{id}/manifest.json");
        self.entry(id, "resource.image", None, &manifest);
        Ok(self)
    }

    /// Change l'empreinte publiée d'un fichier, qui n'est donc plus accepté.
    pub fn publish_with_digests(self, digests: &Value) -> anyhow::Result<PathBuf> {
        write(
            &self.root.join("index.json"),
            &json!({
                "schemaVersion": "1.0.0", "filesSha256": digests, "entries": self.entries
            })
            .to_string(),
        )?;
        Ok(self.root)
    }

    pub fn publish(self) -> anyhow::Result<PathBuf> {
        self.publish_with_digests(&json!({}))
    }
}
