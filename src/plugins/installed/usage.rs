//! Ce dont un flux, une tâche, un modèle ou un élément se sert (ADR 0029). La reprise des dépôts, la remise
//! d'un bail et le refus de désinstaller s'appuient sur les mêmes règles.

use super::DamagedItem;
use crate::{
    error::{AppError, AppResult},
    flows::{OperationContract, OperationRegistry, OperationRuntime, contract::library_script_id},
    plugins::{
        CatalogCache, CatalogEntry, CatalogItemType, catalog_with_roots, flow_block_catalog,
        load_manifest, parse_index_tolerant, rename_legacy_block_kinds, required_probe,
        sources::INDEX_RELATIVE_PATH,
    },
    repository,
    software_probe::declared_probe_tools,
};
use forgearr_shared::{
    flows::{FlowGraph, OwnedBlockSpec, find_owned},
    plugins::store::usage::ItemUsage,
};
use serde_json::Value;
use sqlx::SqliteConnection;
use std::{
    collections::BTreeSet,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// Entrées, blocs et opérations d'un jeu de dossiers.
pub struct EngineCatalog {
    pub entries: Vec<(PathBuf, CatalogEntry)>,
    pub blocks: Vec<OwnedBlockSpec>,
    pub operations: OperationRegistry,
}

impl EngineCatalog {
    /// Lit le disque, sans le cache. Bloquant.
    pub fn read(roots: &[PathBuf]) -> Self {
        Self {
            entries: catalog_with_roots(roots),
            blocks: flow_block_catalog(roots),
            operations: OperationRegistry::load_from_roots(roots),
        }
    }

    pub async fn cached(cache: &CatalogCache, roots: &[PathBuf]) -> Self {
        Self {
            entries: cache.entries(roots).await,
            blocks: cache.blocks(roots).await,
            operations: cache.operations(roots).await,
        }
    }

    pub fn entry(&self, id: &str) -> Option<&CatalogEntry> {
        self.located(id).map(|(_, entry)| entry)
    }

    fn located(&self, id: &str) -> Option<&(PathBuf, CatalogEntry)> {
        self.entries.iter().find(|(_, entry)| entry.id == id)
    }

    fn has_primary_probe(&self) -> bool {
        self.operations.primary_probe().is_some() || self.probe_tool().is_some()
    }

    /// Outil qui sonde les sources quand aucune opération ne le fait.
    fn probe_tool(&self) -> Option<String> {
        if !self.operations.all_probes().is_empty() {
            return None;
        }
        let entries: Vec<CatalogEntry> = self
            .entries
            .iter()
            .map(|(_, entry)| entry.clone())
            .collect();
        required_probe(&entries).map(|probe| probe.tool_id)
    }
}

/// Éléments désignés, présents ou non dans le catalogue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsedElements {
    /// Plugins et scripts.
    pub items: BTreeSet<String>,
    pub tools: BTreeSet<String>,
    /// Types de nœuds sans opération connue.
    pub unknown_kinds: BTreeSet<String>,
}

impl UsedElements {
    fn contains(&self, id: &str) -> bool {
        self.items.contains(id) || self.tools.contains(id)
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.items.extend(other.items);
        self.tools.extend(other.tools);
        self.unknown_kinds.extend(other.unknown_kinds);
    }

    fn add_operation(&mut self, operation: &OperationContract, catalog: &EngineCatalog) {
        self.items.extend(
            catalog
                .operations
                .owner_of(&operation.id)
                .map(str::to_owned),
        );
        self.tools
            .extend(operation.tool_refs().map(|tool| tool.id.clone()));
        match &operation.runtime {
            OperationRuntime::Http(http) => self.items.extend(http.settings_plugin_id.clone()),
            OperationRuntime::Script(script) if !script.dependencies_unverified => {
                self.items.extend(
                    script
                        .requires
                        .iter()
                        .map(|requirement| requirement.provider.clone()),
                )
            }
            _ => {}
        }
    }
}

/// Éléments dont les nœuds de ce graphe se servent.
pub fn elements_used_by(graph: &FlowGraph, catalog: &EngineCatalog) -> UsedElements {
    let mut used = UsedElements::default();
    for node in &graph.nodes {
        match catalog.operations.resolve_node_operation(node) {
            Some(operation) => used.add_operation(operation, catalog),
            None => {
                used.unknown_kinds.insert(node.kind.as_str().to_owned());
            }
        }
        used.items
            .extend(find_owned(&catalog.blocks, &node.kind).and_then(|block| block.owner.clone()));
        if let Some(script_id) = library_script_id(node) {
            let is_item = catalog
                .entry(script_id)
                .is_some_and(|entry| entry.entry_type.is_installed_by_itself());
            if is_item {
                used.items.insert(script_id.to_owned());
            }
            if let Some(operation) = catalog.operations.library_script_operation(node) {
                used.add_operation(operation, catalog);
            }
        }
    }
    used
}

/// Éléments qui entrent dans toute tâche : sondes, opération par défaut et sondes de capacités. Bloquant.
pub(super) fn base_elements(catalog: &EngineCatalog) -> UsedElements {
    let operations = &catalog.operations;
    let mut used = UsedElements::default();
    used.tools.extend(catalog.probe_tool());
    for operation in operations
        .all_probes()
        .into_iter()
        .chain(operations.default_fallback_operation())
    {
        used.add_operation(operation, catalog);
    }
    for (root, entry) in &catalog.entries {
        if !declared_probe_tools(root, entry).is_empty() {
            used.items.insert(entry.id.clone());
        }
    }
    used
}

/// L'élément de ce dossier porte-t-il une sonde ou une opération par défaut ? Bloquant.
pub(super) fn serves_every_task(root: &Path) -> bool {
    let operations = OperationRegistry::load_from_roots(&[root.to_path_buf()]);
    !operations.all_probes().is_empty() || operations.default_fallback_operation().is_some()
}

/// Manque-t-il au catalogue de quoi sonder ou traiter une tâche sans flux ?
pub(super) fn base_is_missing(catalog: &EngineCatalog) -> bool {
    !catalog.has_primary_probe() || catalog.operations.default_fallback_operation().is_none()
}

/// Éléments dont un élément du catalogue se sert lui-même : dépendances déclarées, opérations, celles de
/// ses blocs et sondes de capacités. Bloquant.
pub(super) fn elements_declared_by(id: &str, catalog: &EngineCatalog) -> UsedElements {
    let mut used = UsedElements::default();
    let Some((root, entry)) = catalog.located(id) else {
        return used;
    };
    let operations = &catalog.operations;
    let of_blocks = catalog
        .blocks
        .iter()
        .filter(|block| block.owner.as_deref() == Some(id))
        .filter_map(|block| operations.get(block.operation_id.as_deref()?));
    for operation in operations
        .operations()
        .filter(|operation| operations.owner_of(&operation.id) == Some(id))
        .chain(of_blocks)
    {
        used.add_operation(operation, catalog);
    }
    used.tools.extend(declared_probe_tools(root, entry));
    let dependencies = load_manifest(root, &entry.manifest_path)
        .map(|manifest| manifest.dependencies)
        .unwrap_or_default();
    for dependency in dependencies {
        match catalog.entry(&dependency.id).map(|entry| entry.entry_type) {
            Some(CatalogItemType::App) => used.tools.insert(dependency.id),
            Some(_) => used.items.insert(dependency.id),
            // Facultative et absente : rien n'en dépend.
            None => continue,
        };
    }
    used.items.remove(id);
    used
}

/// Ajoute, de proche en proche, ce dont ces éléments se servent eux-mêmes. Bloquant.
pub(super) fn with_declared_elements(
    mut used: UsedElements,
    catalog: &EngineCatalog,
) -> UsedElements {
    let mut visited = BTreeSet::new();
    let mut waiting: Vec<String> = used.items.union(&used.tools).cloned().collect();
    while let Some(id) = waiting.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        let declared = elements_declared_by(&id, catalog);
        waiting.extend(
            declared
                .items
                .union(&declared.tools)
                .filter(|id| !visited.contains(*id))
                .cloned(),
        );
        used.extend(declared);
    }
    used
}

/// Élément installé illisible (`damaged`) dont une tâche dépend. Sa copie ne dit plus ce qu'il portait : un
/// nœud sans opération, ou un socle incomplet, lui est attribué, car attendre la réparation ne perd rien. Ce
/// qui sert à toute tâche n'est nommé par aucun nœud : l'inscription le dit. `None` : tâche sans flux.
pub(super) fn unreadable_dependency<'a>(
    damaged: &'a [DamagedItem],
    catalog: &EngineCatalog,
    graphs: &[Option<FlowGraph>],
) -> Option<&'a str> {
    // Un autre dossier du moteur peut encore donner l'élément.
    let absent: Vec<&DamagedItem> = damaged
        .iter()
        .filter(|item| catalog.entry(&item.id).is_none())
        .collect();
    let first = absent.first()?.id.as_str();
    if let Some(base) = absent.iter().find(|item| item.serves_every_task) {
        return Some(&base.id);
    }
    if base_is_missing(catalog) {
        return Some(first);
    }
    let mut used = UsedElements::default();
    for operation in catalog.operations.all_probes() {
        used.add_operation(operation, catalog);
    }
    for graph in graphs {
        match graph {
            Some(graph) => used.extend(elements_used_by(graph, catalog)),
            None => used.extend(default_elements(catalog)),
        }
    }
    if !used.unknown_kinds.is_empty() {
        return Some(first);
    }
    absent
        .into_iter()
        .map(|item| item.id.as_str())
        .find(|id| used.contains(id))
}

/// Éléments d'une tâche sans flux.
fn default_elements(catalog: &EngineCatalog) -> UsedElements {
    let mut used = UsedElements::default();
    if let Some(operation) = catalog.operations.default_fallback_operation() {
        used.add_operation(operation, catalog);
    }
    used
}

/// Graphe nommé.
struct NamedGraph {
    name: String,
    graph: FlowGraph,
}

/// Tâches non terminées d'une même version de flux ; `None` : sans flux.
struct TaskGraph {
    sources: Vec<String>,
    graph: Option<FlowGraph>,
}

/// Tout ce qui peut se servir d'un élément.
pub struct UsageSources {
    flows: Vec<NamedGraph>,
    tasks: Vec<TaskGraph>,
    templates: Vec<NamedGraph>,
}

impl UsageSources {
    pub async fn read(conn: &mut SqliteConnection, local_dir: &Path) -> AppResult<Self> {
        let flows = repository::flow_definitions(conn).await?;
        let tasks = repository::unfinished_flows(conn).await?;
        let local_dir = local_dir.to_path_buf();
        let templates = tokio::task::spawn_blocking(move || local_templates(&local_dir))
            .await
            .map_err(|_| AppError::Internal)??;
        Ok(Self {
            flows: flows
                .into_iter()
                .map(|flow| NamedGraph {
                    graph: parse_graph(&flow.definition),
                    name: flow.name,
                })
                .collect(),
            tasks: tasks
                .into_iter()
                .map(|task| TaskGraph {
                    graph: task.definition.as_deref().map(parse_graph),
                    sources: task.sources,
                })
                .collect(),
            templates,
        })
    }

    /// Tous les graphes.
    pub(super) fn graphs(&self) -> impl Iterator<Item = &FlowGraph> {
        self.flows
            .iter()
            .chain(&self.templates)
            .map(|named| &named.graph)
            .chain(self.tasks.iter().filter_map(|task| task.graph.as_ref()))
    }
}

/// Les types de blocs d'avant le catalogue 2.0.0 sont renommés comme au démarrage.
pub(super) fn parse_graph(definition: &str) -> FlowGraph {
    let renamed = rename_legacy_block_kinds(definition);
    FlowGraph::parse(renamed.as_deref().unwrap_or(definition))
}

/// Entrées de la collection locale ; aucune si elle n'a pas encore d'index. Un index illisible est une erreur,
/// pas une collection vide, dont rien ne se servirait. Bloquant.
pub(super) fn local_entries(local_dir: &Path) -> AppResult<Vec<CatalogEntry>> {
    let Some(text) = read_if_present(&local_dir.join(INDEX_RELATIVE_PATH))? else {
        return Ok(Vec::new());
    };
    parse_index_tolerant(&text)
        .map(|parsed| parsed.index.entries)
        .map_err(|error| AppError::InvalidManifest(format!("collection locale : {error}")))
}

fn read_if_present(path: &Path) -> AppResult<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Modèles de flux et de bloc de la collection locale.
fn local_templates(local_dir: &Path) -> AppResult<Vec<NamedGraph>> {
    let mut templates = Vec::new();
    for entry in local_entries(local_dir)? {
        let Some(text) = read_if_present(&local_dir.join(&entry.manifest_path))? else {
            continue;
        };
        let document: Value = match serde_json::from_str(&text) {
            Ok(document) => document,
            Err(error) => {
                tracing::warn!(item = %entry.id, %error, "élément local illisible, ignoré");
                continue;
            }
        };
        let definition = match document.get("definition") {
            Some(Value::String(definition)) => definition.clone(),
            Some(definition) => definition.to_string(),
            None => continue,
        };
        templates.push(NamedGraph {
            graph: parse_graph(&definition),
            name: entry.name,
        });
    }
    Ok(templates)
}

/// Ce qui se sert d'un élément installé, et empêche donc de le désinstaller. Bloquant.
pub fn item_usage(
    item_id: &str,
    catalog: &EngineCatalog,
    sources: &UsageSources,
    local_dir: &Path,
) -> ItemUsage {
    let operations = &catalog.operations;
    // Les sondes tournent avant tout flux : leurs outils servent à chaque tâche.
    let is_probe_tool = operations
        .all_probes()
        .into_iter()
        .flat_map(|operation| operation.tool_refs())
        .any(|tool| tool.id == item_id);
    let uses = |graph: &FlowGraph| elements_used_by(graph, catalog).contains(item_id);
    let names = |graphs: &[NamedGraph], always: bool| {
        graphs
            .iter()
            .filter(|named| always || uses(&named.graph))
            .map(|named| named.name.clone())
            .collect::<Vec<String>>()
    };
    let jobs: BTreeSet<String> = sources
        .tasks
        .iter()
        .filter(|task| {
            is_probe_tool
                || task
                    .graph
                    .as_ref()
                    .map_or_else(|| default_elements(catalog).tools.contains(item_id), &uses)
        })
        .flat_map(|task| task.sources.clone())
        .collect();
    let scripts = catalog
        .entries
        .iter()
        .filter(|(root, entry)| {
            root == local_dir && entry.entry_type == CatalogItemType::Script && entry.id != item_id
        })
        .filter(|(_, entry)| elements_declared_by(&entry.id, catalog).contains(item_id))
        .map(|(_, entry)| entry.name.clone())
        .collect();
    let is_last = |plays: fn(&OperationContract) -> bool| {
        let holders: BTreeSet<&str> = operations
            .operations()
            .filter(|operation| plays(operation))
            .filter_map(|operation| operations.owner_of(&operation.id))
            .collect();
        holders == BTreeSet::from([item_id])
    };
    ItemUsage {
        flows: names(&sources.flows, is_probe_tool),
        jobs: jobs.into_iter().collect(),
        templates: names(&sources.templates, false),
        scripts,
        last_primary_probe: is_last(OperationRegistry::is_primary_probe),
        last_default_operation: is_last(OperationRegistry::is_default_operation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::UpdateFlowRequest,
        plugins::installed::test_support::{Repository, write},
    };
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Graphe d'un seul nœud.
    fn graph(kind: &str, config: Value) -> FlowGraph {
        let node = json!({"id": "n", "type": kind, "name": "N", "config": config});
        FlowGraph::parse(&json!({"nodes": [node], "connections": []}).to_string())
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn catalog_of(repository: &Repository) -> EngineCatalog {
        EngineCatalog::read(&[repository.path().to_path_buf()])
    }

    /// Socle porté par `socle`, hors de tout dossier.
    fn add_base(catalog: &mut EngineCatalog) -> anyhow::Result<()> {
        for (id, runtime) in [
            ("operation.probe", json!({"executionMode": "probe-primary"})),
            (
                "operation.default",
                json!({"executionMode": "staged-output", "defaultFallback": true}),
            ),
        ] {
            let mut runtime = runtime;
            runtime["kind"] = json!("binary");
            runtime["executable"] = json!("outil");
            catalog.operations.register_json(json!({
                "schemaVersion": "1.1.0", "id": id, "version": "1.0.0", "name": id,
                "providerId": "socle", "providerKind": "plugin", "capability": "x",
                "status": "validated", "runtime": runtime
            }))?;
        }
        Ok(())
    }

    #[test]
    fn a_node_leads_to_its_plugin_its_tools_and_the_script_it_names() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let catalog = catalog_of(&repository);
        for kind in ["demo.run", "demo-run"] {
            let used = elements_used_by(&graph(kind, Value::Null), &catalog);
            assert_eq!(used.items, set(&["demo"]), "{kind}");
            assert_eq!(used.tools, set(&["outil"]), "{kind}");
            assert!(used.unknown_kinds.is_empty());
        }
        let unknown = elements_used_by(&graph("absent.run", Value::Null), &catalog);
        assert_eq!(unknown.unknown_kinds, set(&["absent.run"]));
        assert!(unknown.items.is_empty());
        // Script de la librairie, par identifiant d'élément ou d'opération.
        for script in ["demo", "demo.run"] {
            let used =
                elements_used_by(&graph("absent.run", json!({"scriptId": script})), &catalog);
            assert!(used.items.contains("demo"), "{script}");
        }
        Ok(())
    }

    #[test]
    fn what_an_element_declares_is_followed() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let catalog = catalog_of(&repository);
        let wanted = UsedElements {
            items: set(&["demo", "absent"]),
            ..UsedElements::default()
        };
        let all = with_declared_elements(wanted, &catalog);
        assert_eq!(all.items, set(&["absent", "demo"]));
        assert_eq!(all.tools, set(&["outil"]));
        Ok(())
    }

    #[test]
    fn a_task_depends_on_an_unreadable_item_it_may_have_used() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let mut catalog = catalog_of(&repository);
        let item = |id: &str, serves_every_task: bool| DamagedItem {
            id: id.to_owned(),
            serves_every_task,
        };
        let damaged = [item("perdu", false), item("demo", false)];
        let known = [Some(graph("demo.run", Value::Null))];
        let unknown = [Some(graph("perdu.run", Value::Null))];
        // Sans sonde ni opération par défaut, l'élément perdu pouvait les porter.
        assert_eq!(
            unreadable_dependency(&damaged, &catalog, &known),
            Some("perdu")
        );
        add_base(&mut catalog)?;
        assert_eq!(unreadable_dependency(&damaged, &catalog, &known), None);
        assert_eq!(unreadable_dependency(&damaged, &catalog, &[None]), None);
        assert_eq!(
            unreadable_dependency(&damaged, &catalog, &unknown),
            Some("perdu")
        );
        // Une sonde d'appoint perdue n'est nommée par aucun nœud.
        let probe = [item("perdu", false), item("sonde", true)];
        assert_eq!(
            unreadable_dependency(&probe, &catalog, &known),
            Some("sonde")
        );
        // Outil dont le contrat figé manque.
        catalog.entries.retain(|(_, entry)| entry.id != "outil");
        let damaged = [item("outil", false)];
        assert_eq!(
            unreadable_dependency(&damaged, &catalog, &known),
            Some("outil")
        );
        assert_eq!(unreadable_dependency(&[], &catalog, &unknown), None);
        Ok(())
    }

    #[test]
    fn an_unreadable_local_collection_is_not_an_empty_one() -> anyhow::Result<()> {
        let local = tempfile::tempdir()?;
        assert!(local_templates(local.path())?.is_empty());
        write(local.path(), INDEX_RELATIVE_PATH, "{")?;
        let refused = local_templates(local.path());
        assert!(matches!(refused, Err(AppError::InvalidManifest(_))));
        // Un seul élément illisible n'empêche pas de lire les autres.
        let entry = |id: &str| json!({"id": id, "type": "flow", "version": "1.0.0", "name": id, "manifestPath": format!("flows/{id}/manifest.json")});
        write(
            local.path(),
            INDEX_RELATIVE_PATH,
            &json!({"schemaVersion": "1.0.0", "entries": [entry("a"), entry("b"), entry("c")]})
                .to_string(),
        )?;
        write(local.path(), "flows/a/manifest.json", "{")?;
        let definition = json!({"definition": {"nodes": [], "connections": []}}).to_string();
        write(local.path(), "flows/b/manifest.json", &definition)?;
        let templates = local_templates(local.path())?;
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].name, "b");
        Ok(())
    }

    #[test]
    fn only_a_probe_or_a_default_operation_serves_every_task() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        assert!(!serves_every_task(repository.path()));
        write(
            repository.path(),
            "plugins/demo/operations/run.json",
            &json!({
                "schemaVersion": "1.1.0", "id": "operation.demo.run", "version": "1.0.0",
                "name": "Demo", "providerId": "demo", "providerKind": "plugin",
                "capability": "x", "status": "validated",
                "runtime": {
                    "kind": "binary", "executable": "outil", "executionMode": "probe-enrich"
                }
            })
            .to_string(),
        )?;
        assert!(serves_every_task(repository.path()));
        Ok(())
    }

    #[tokio::test]
    async fn the_usage_of_an_item_names_flows_tasks_templates_and_scripts() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        let definition = json!({
            "nodes": [{"id": "n", "type": "demo-run", "name": "N"}], "connections": []
        });
        let flow = UpdateFlowRequest {
            name: "Mon flux".to_owned(),
            description: None,
            file_mask: "*".to_owned(),
            is_default: true,
            definition: definition.to_string(),
        };
        repository::save_flow(&pool, "flux.a", &flow).await?;
        repository::insert_job(&pool, "film.mkv", None, &"ab".repeat(32), 1).await?;

        let repository = Repository::with_plugin()?;
        let local = tempfile::tempdir()?;
        let entry = |id: &str, kind: &str, path: &str| json!({"id": id, "type": kind, "version": "1.0.0", "name": id, "manifestPath": path});
        write(
            local.path(),
            "index.json",
            &json!({"schemaVersion": "1.0.0", "entries": [
                entry("Modèle", "flow", "flows/a/manifest.json"),
                entry("Script", "script", "scripts/a/manifest.json"),
            ]})
            .to_string(),
        )?;
        write(
            local.path(),
            "flows/a/manifest.json",
            &json!({"definition": definition}).to_string(),
        )?;
        write(
            local.path(),
            "scripts/a/manifest.json",
            &json!({
                "schemaVersion": "1.1.0", "id": "Script", "type": "script", "version": "1.0.0",
                "name": "Script", "dependencies": [{"id": "demo", "kind": "plugin"}]
            })
            .to_string(),
        )?;
        let roots = [local.path().to_path_buf(), repository.path().to_path_buf()];
        let mut catalog = EngineCatalog::read(&roots);
        let mut conn = pool.acquire().await?;
        let sources = UsageSources::read(&mut conn, local.path()).await?;
        let usage = item_usage("demo", &catalog, &sources, local.path());
        assert_eq!(usage.flows, ["Mon flux"]);
        assert_eq!(usage.jobs, ["film.mkv"]);
        assert_eq!(usage.templates, ["Modèle"]);
        assert_eq!(usage.scripts, ["Script"]);
        assert!(!usage.last_primary_probe && !usage.last_default_operation);
        assert_eq!(
            item_usage("outil", &catalog, &sources, local.path()).flows,
            ["Mon flux"]
        );
        assert!(item_usage("autre", &catalog, &sources, local.path()).is_empty());
        // Un registre reçu dans un bail n'a pas de propriétaires : rien n'est attribué à tort.
        add_base(&mut catalog)?;
        assert!(!item_usage("demo", &catalog, &sources, local.path()).last_primary_probe);
        Ok(())
    }
}
