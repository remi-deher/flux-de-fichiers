//! Éléments de dépôt, décrits en une expression.

use serde_json::{Value, json};
use std::path::Path;

/// Version du contrat d'un outil : celle de son entrée.
pub(super) const FIRST_VERSION: &str = "1.0.0";

#[derive(Clone)]
pub(super) struct Item {
    kind: &'static str,
    pub id: String,
    version: String,
    manifest: Value,
    operations: Vec<Value>,
    blocks: Vec<Value>,
    files: Vec<(String, String)>,
    omitted: Vec<String>,
}

pub(super) fn primitive() -> Value {
    json!({"kind": "primitive", "primitive": "entry"})
}

pub(super) fn binary(tool: &str) -> Value {
    json!({
        "kind": "binary", "executable": tool, "tool": {"id": tool}, "executionMode": "staged-output"
    })
}

/// Sonde qui traite aussi les tâches sans flux.
pub(super) fn base() -> Value {
    json!({
        "kind": "binary", "executable": "sonde", "executionMode": "probe-primary",
        "defaultFallback": true
    })
}

/// Opération d'API qui lit les réglages de ce plugin.
pub(super) fn reading_settings_of(plugin: &str) -> Value {
    json!({"kind": "http", "settingsPluginId": plugin, "steps": []})
}

pub(super) fn script_requiring(requires: Value) -> Value {
    json!({
        "kind": "script", "interpreter": "sh", "code": "true", "requires": requires,
        "tool": {"id": "interpreteurs", "version": "1.1.0"}
    })
}

impl Item {
    pub fn new(kind: &'static str, id: &str) -> Self {
        Self {
            kind,
            id: id.to_owned(),
            version: FIRST_VERSION.to_owned(),
            manifest: json!({}),
            operations: Vec::new(),
            blocks: Vec::new(),
            files: Vec::new(),
            omitted: Vec::new(),
        }
    }

    /// Plugin d'opération `operation.<id>.run` et de bloc `<id>.run` (alias `<id>-run`).
    pub fn plugin(id: &str, runtime: Value) -> Self {
        Self::new("plugin", id).running(&format!("operation.{id}.run"), runtime)
    }

    /// Script d'opération `operation.script.<id>`.
    pub fn script(id: &str, runtime: Value) -> Self {
        Self::new("script", id).running(&format!("operation.script.{id}"), runtime)
    }

    /// Outil téléchargeable sur cet hôte.
    pub fn tool(id: &str) -> Self {
        Self::new("app", id)
    }

    /// Flux d'un nœud par type de bloc.
    pub fn flow(id: &str, kinds: &[&str]) -> Self {
        let nodes: Vec<Value> = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| json!({"id": format!("n{index}"), "type": kind, "name": kind}))
            .collect();
        Self::new("flow", id).with("definition", json!({"nodes": nodes, "connections": []}))
    }

    /// Pack de ces composants (type, identifiant, version minimale).
    pub fn pack(id: &str, components: &[(&str, &str, &str)]) -> Self {
        let components: Vec<Value> = components
            .iter()
            .map(|(kind, id, version)| json!({"kind": kind, "id": id, "version": version}))
            .collect();
        let installation = json!({
            "schemaVersion": "1.0.0", "packId": id, "packVersion": FIRST_VERSION,
            "kind": "declarative-bundle", "components": components
        });
        let mut pack = Self::new("pack", id);
        pack.files
            .push(("installation.json".to_owned(), installation.to_string()));
        pack
    }

    fn running(self, operation_id: &str, runtime: Value) -> Self {
        let id = self.id.clone();
        self.operation(operation_id, runtime).block(
            &format!("{id}.run"),
            &format!("{id}-run"),
            operation_id,
        )
    }

    pub fn operation(mut self, operation_id: &str, runtime: Value) -> Self {
        self.operations.push(json!({
            "schemaVersion": "1.1.0", "id": operation_id, "version": "1.0.0", "name": operation_id,
            "providerId": self.id, "providerKind": self.kind, "capability": "custom",
            "status": "validated", "runtime": runtime
        }));
        self
    }

    pub fn block(mut self, kind: &str, alias: &str, operation_id: &str) -> Self {
        self.blocks.push(json!({
            "schemaVersion": "1.1.0", "id": kind, "version": "1.0.0", "name": kind,
            "aliases": [alias], "operationId": operation_id
        }));
        self
    }

    /// Change un champ de la dernière opération.
    pub fn declaring(mut self, field: &str, value: Value) -> Self {
        if let Some(operation) = self.operations.last_mut() {
            operation[field] = value;
        }
        self
    }

    pub fn exposing(self, namespace: &str, contract_version: &str, names: &[&str]) -> Self {
        let variables: Vec<Value> = names
            .iter()
            .map(|name| json!({"name": name, "path": name}))
            .collect();
        self.declaring(
            "exposes",
            json!({
                "namespace": namespace, "contractVersion": contract_version, "variables": variables
            }),
        )
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = version.to_owned();
        self
    }

    pub fn with(mut self, field: &str, value: Value) -> Self {
        self.manifest[field] = value;
        self
    }

    pub fn depending(self, dependencies: Value) -> Self {
        self.with("dependencies", dependencies)
    }

    /// Dépendance facultative à ces outils, comme les plugins officiels.
    pub fn recommending(self, tools: &[&str]) -> Self {
        let dependencies: Vec<Value> = tools
            .iter()
            .map(|id| json!({"id": id, "versionRange": "^1.0.0", "kind": "app", "optional": true}))
            .collect();
        self.depending(Value::Array(dependencies))
    }

    /// Fichier déclaré que le dépôt n'a pas.
    pub fn omitting(mut self, path: &str) -> Self {
        self.omitted.push(path.to_owned());
        self
    }

    fn folder(&self) -> String {
        format!("{}s/{}", self.kind, self.id)
    }

    /// Écrit les fichiers, et rend les entrées d'index.
    pub fn write(&self, root: &Path) -> anyhow::Result<Vec<Value>> {
        let folder = self.folder();
        let entry = |id: &str, kind: &str, path: String| json!({"id": id, "type": kind, "version": self.version, "name": id, "manifestPath": path});
        let mut entries = vec![entry(
            &self.id,
            self.kind,
            format!("{folder}/manifest.json"),
        )];
        let mut declared = Vec::new();
        let mut files = self.files.clone();
        for (index, operation) in self.operations.iter().enumerate() {
            let path = format!("operations/{index}.json");
            declared.push(json!({"path": path, "role": "operation"}));
            files.push((path, operation.to_string()));
        }
        for (index, block) in self.blocks.iter().enumerate() {
            let path = format!("blocks/{index}.json");
            declared.push(json!({"path": path, "role": "block"}));
            let id = block["id"].as_str().unwrap_or_default();
            let mut entry = entry(id, "block", format!("{folder}/{path}"));
            entry["owner"] = json!(self.id);
            entries.push(entry);
            files.push((path, block.to_string()));
        }
        if self.kind == "app" {
            let artifact = json!({
                "arch": std::env::consts::ARCH, "archiveUrl": "https://example.org/x.tar.gz",
                "archiveFormat": "tar.gz", "archiveSha256": "", "binaryRelativePath": "x"
            });
            let contract = json!({
                "toolId": self.id, "toolVersion": self.version, "artifacts": [artifact]
            });
            files.push(("distribution.v1.json".to_owned(), contract.to_string()));
        }
        let mut manifest = json!({
            "schemaVersion": "1.1.0", "id": self.id, "type": self.kind, "version": self.version,
            "name": self.id, "files": declared
        });
        for (field, value) in self.manifest.as_object().into_iter().flatten() {
            manifest[field] = value.clone();
        }
        files.push(("manifest.json".to_owned(), manifest.to_string()));
        for (path, content) in files {
            if !self.omitted.contains(&path) {
                let target = root.join(&folder).join(path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(target, content)?;
            }
        }
        Ok(entries)
    }
}
