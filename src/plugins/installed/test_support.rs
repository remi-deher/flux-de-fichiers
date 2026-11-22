//! Dépôt de test : un plugin `demo` (opération, code, bloc) qui lance l'outil `outil`, cet outil et un pack.

use serde_json::{Value, json};
use std::path::Path;

const INDEX: &str = "index.json";
pub(super) const OPERATION: &str = "operation.demo.run";
/// Code de [`OPERATION`].
pub(super) const CODE: &str = "print(1)";

pub(super) fn write(root: &Path, path: &str, content: &str) -> std::io::Result<()> {
    let target = root.join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, content)
}

pub(super) struct Repository {
    dir: tempfile::TempDir,
}

impl Repository {
    pub(super) fn with_plugin() -> anyhow::Result<Self> {
        Self::with_plugin_in("plugins/demo")
    }

    /// Le plugin `demo` dans ce dossier du dépôt.
    pub(super) fn with_plugin_in(folder: &str) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        let manifest = json!({
            "schemaVersion": "1.1.0", "id": "demo", "type": "plugin", "version": "1.0.0", "name": "Demo",
            "files": [
                {"path": "operations/run.json", "role": "operation"},
                {"path": "blocks/run.json", "role": "block"},
                {"path": "run.py", "role": "script"},
                {"path": "deploy.sh", "role": "deploy"},
                {"path": "README.md", "role": "docs"}
            ]
        });
        let operation = json!({
            "schemaVersion": "1.1.0", "id": OPERATION, "version": "1.0.0", "name": "Demo",
            "providerId": "demo", "providerKind": "plugin", "capability": "custom", "status": "validated",
            "runtime": {
                "kind": "script", "interpreter": "sh", "entrypoint": "run.py", "tool": {"id": "outil"}
            }
        });
        let block = json!({
            "id": "demo.run", "name": "Demo", "aliases": ["demo-run"], "operationId": OPERATION
        });
        write(
            root,
            &format!("{folder}/manifest.json"),
            &manifest.to_string(),
        )?;
        write(
            root,
            &format!("{folder}/operations/run.json"),
            &operation.to_string(),
        )?;
        write(
            root,
            &format!("{folder}/blocks/run.json"),
            &block.to_string(),
        )?;
        write(root, &format!("{folder}/run.py"), CODE)?;
        write(root, "apps/outil/manifest.json", "{}")?;
        let entry = |id: &str, kind: &str, path: String| json!({"id": id, "type": kind, "version": "1.0.0", "name": id, "manifestPath": path});
        let mut block = entry("demo.run", "block", format!("{folder}/blocks/run.json"));
        block["owner"] = json!("demo");
        let index = json!({"schemaVersion": "1.0.0", "entries": [
            entry("demo", "plugin", format!("{folder}/manifest.json")),
            block,
            entry("outil", "app", "apps/outil/manifest.json".to_owned()),
            entry("pack", "pack", "packs/pack/manifest.json".to_owned()),
        ]});
        write(root, INDEX, &index.to_string())?;
        Ok(Self { dir })
    }

    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    pub(super) fn edit_index(&self, edit: impl FnOnce(&mut Value)) -> anyhow::Result<()> {
        let path = self.path().join(INDEX);
        let mut index: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        edit(&mut index);
        std::fs::write(path, index.to_string())?;
        Ok(())
    }

    /// Change un champ de l'entrée `id` de l'index.
    pub(super) fn edit_entry(&self, id: &str, field: &str, value: Value) -> anyhow::Result<()> {
        self.edit_index(|index| {
            let entries = index["entries"].as_array_mut();
            for entry in entries.into_iter().flatten() {
                if entry["id"] == id {
                    entry[field] = value.clone();
                }
            }
        })
    }
}
