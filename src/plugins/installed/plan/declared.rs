//! Ce qu'un élément déclare, lu dans son propre contenu : un autre élément de son dépôt peut porter les mêmes
//! noms.

use crate::{
    flows::OperationContract,
    plugins::{
        CatalogEntry, CatalogItemType, PluginDependency, PluginManifest, SettingsField,
        blocks::block_spec,
        install::DISTRIBUTION_FILE_NAME,
        installed::{
            ItemContent,
            files::{DeclaredRole, declared_files},
            paths::{declared_path, folder_of, safe_path},
            usage::{document_graph, requirements_of},
        },
        parse_distribution,
    },
};
use forgearr_shared::{
    exposed_variables::{ExposedVariables, ScriptRequirement},
    flows::{FlowGraph, OwnedBlockSpec},
};
use serde::Deserialize;
use serde_json::Value;

/// Opération que le moteur saurait lire.
#[derive(Clone, Debug)]
pub(super) struct DeclaredOperation {
    pub raw: Value,
    pub contract: OperationContract,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Declared {
    /// Version qu'une exigence compare : celle du contrat pour un outil.
    pub version: String,
    /// L'élément et ses blocs, tels que l'index les nomme.
    pub entry_ids: Vec<String>,
    pub dependencies: Vec<PluginDependency>,
    pub operations: Vec<DeclaredOperation>,
    /// Opérations que le moteur ne saurait pas lire.
    pub unreadable: Vec<String>,
    pub blocks: Vec<OwnedBlockSpec>,
    /// Outils qu'interrogent les sondes de capacités.
    pub probe_tools: Vec<String>,
    pub settings: Vec<SettingsField>,
    /// Définition d'un flux.
    pub graph: Option<FlowGraph>,
}

impl Declared {
    /// `Err` explique pourquoi le moteur ne saurait rien faire de cet élément.
    pub fn of(content: &ItemContent) -> Result<Self, String> {
        let entry = &content.entry;
        let mut declared = Self {
            version: entry.version.clone(),
            entry_ids: vec![entry.id.clone()],
            ..Self::default()
        };
        for block in block_entries(content) {
            declared.blocks.extend(
                text(content, &block.manifest_path).and_then(|text| block_spec(&block, text)),
            );
            declared.entry_ids.push(block.id);
        }
        let manifest = safe_path(&entry.manifest_path).unwrap_or_default();
        // Un manifeste absent est déjà dans `missing`.
        let Some(document) = json(content, &manifest) else {
            return Ok(declared);
        };
        let folder = folder_of(&manifest);
        match entry.entry_type {
            CatalogItemType::Plugin | CatalogItemType::Script => {
                declared.read_engine_files(content, &document, folder)?;
            }
            kind => {
                declared.dependencies = readable_dependencies(&document);
                match kind {
                    CatalogItemType::App => declared.read_contract(content)?,
                    CatalogItemType::Flow => declared.read_graph(&document)?,
                    _ => {}
                }
            }
        }
        Ok(declared)
    }

    /// Le moteur n'ouvre ces éléments que si tout leur manifeste est lisible.
    fn read_engine_files(
        &mut self,
        content: &ItemContent,
        document: &Value,
        folder: &str,
    ) -> Result<(), String> {
        let manifest = PluginManifest::deserialize(document)
            .map_err(|error| format!("manifeste illisible : {error}"))?;
        self.dependencies = manifest.dependencies;
        self.settings = manifest
            .settings_schema
            .map(|schema| schema.fields)
            .unwrap_or_default();
        for (role, path) in declared_files(document) {
            let Some(path) = declared_path(folder, path) else {
                continue;
            };
            match role {
                Some(DeclaredRole::Operation) if content.files().contains_key(&path) => {
                    let operation = json(content, &path).and_then(|raw| {
                        let contract = OperationContract::parse(&raw).ok()?;
                        Some(DeclaredOperation { raw, contract })
                    });
                    match operation {
                        Some(operation) => self.operations.push(operation),
                        None => self.unreadable.push(path),
                    }
                }
                Some(DeclaredRole::CapabilityProbe) => self.probe_tools.extend(
                    json(content, &path)
                        .and_then(|probe| probe.get("toolId")?.as_str().map(str::to_owned)),
                ),
                _ => {}
            }
        }
        Ok(())
    }

    fn read_contract(&mut self, content: &ItemContent) -> Result<(), String> {
        let Some(path) = content
            .files()
            .keys()
            .find(|path| path.ends_with(DISTRIBUTION_FILE_NAME))
        else {
            // Absent : déjà dans `missing`.
            return Ok(());
        };
        let contract = parse_distribution(text(content, path).unwrap_or_default())
            .map_err(|error| format!("contrat illisible : {error}"))?;
        if contract.tool_id != content.entry.id {
            return Err(format!("son contrat décrit « {} »", contract.tool_id));
        }
        let version = contract.tool_version.trim();
        if !version.is_empty() {
            self.version = version.to_owned();
        }
        Ok(())
    }

    fn read_graph(&mut self, document: &Value) -> Result<(), String> {
        let graph = document_graph(document).filter(|graph| !graph.nodes.is_empty());
        self.graph = Some(graph.ok_or("définition de flux absente, vide ou illisible")?);
        Ok(())
    }

    pub fn exposures(&self) -> impl Iterator<Item = &ExposedVariables> {
        self.operations
            .iter()
            .filter_map(|operation| operation.contract.exposes.as_ref())
    }

    /// Variables que ses scripts exigent d'autres éléments.
    pub fn requirements(&self) -> impl Iterator<Item = &ScriptRequirement> {
        self.operations
            .iter()
            .flat_map(|operation| requirements_of(&operation.contract))
    }

    pub fn operation(&self, id: &str) -> Option<&DeclaredOperation> {
        self.operations
            .iter()
            .find(|operation| operation.contract.id == id)
    }
}

fn block_entries(content: &ItemContent) -> impl Iterator<Item = CatalogEntry> {
    content
        .raw_entries
        .iter()
        .filter_map(|raw| CatalogEntry::deserialize(raw).ok())
        .filter(|entry| entry.entry_type == CatalogItemType::Block)
}

fn text<'a>(content: &'a ItemContent, path: &str) -> Option<&'a str> {
    let bytes = content.files().get(&safe_path(path)?)?;
    std::str::from_utf8(bytes).ok()
}

fn json(content: &ItemContent, path: &str) -> Option<Value> {
    serde_json::from_str(text(content, path)?).ok()
}

/// Dépendances d'un manifeste que le moteur n'ouvre pas en entier ; une illisible est ignorée.
fn readable_dependencies(document: &Value) -> Vec<PluginDependency> {
    let declared = document.get("dependencies").and_then(Value::as_array);
    declared
        .into_iter()
        .flatten()
        .filter_map(|dependency| PluginDependency::deserialize(dependency).ok())
        .collect()
}
