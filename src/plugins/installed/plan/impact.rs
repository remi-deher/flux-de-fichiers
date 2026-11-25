//! Ce qu'une mise à jour retire à ce qui s'en sert : éléments installés, flux, modèles et tâches non terminées.

use super::{
    after::engine_changes,
    context::PlanContext,
    findings::Findings,
    needs::stated_tools,
    walk::{Change, Selected},
};
use crate::{
    flows::{OperationContract, check_references},
    plugins::{
        CatalogEntry,
        installed::{EngineCatalog, usage::requirements_of},
        load_manifest,
    },
};
use forgearr_shared::{
    exposed_variables::ExposedVariables,
    flows::{FlowGraph, find_owned},
    plugins::store::plan::{PlanBlocker, PlanConsent},
    version::satisfies,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

/// Élément qui portait un nom disparu, et ce nom.
type Lost = (String, String);

pub(super) fn check_impact(
    context: &PlanContext,
    selected: &[Selected],
    after: &EngineCatalog,
    findings: &mut Findings,
) {
    let updates: Vec<&Change> = engine_changes(selected)
        .filter(|change| change.previous.is_some())
        .collect();
    if updates.is_empty() {
        return;
    }
    check_dependents(context, &updates, after, findings);
    check_graphs(context, after, findings);
    check_base(context, &updates, after, findings);
    check_exposures(context, &updates, after, findings);
}

/// Éléments hors du plan qui ne pourraient plus se servir d'un élément mis à jour. Ceux qui ne le pouvaient déjà plus
/// n'y changent rien.
fn check_dependents(
    context: &PlanContext,
    updates: &[&Change],
    after: &EngineCatalog,
    findings: &mut Findings,
) {
    let versions: HashMap<&str, (&str, &str)> = updates
        .iter()
        .filter_map(|change| {
            let previous = change.previous.as_ref()?;
            let versions = (previous.version.as_str(), change.declared.version.as_str());
            Some((change.content.entry.id.as_str(), versions))
        })
        .collect();
    let engine = &context.engine;
    let mut broken: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (root, entry) in &engine.entries {
        if versions.contains_key(entry.id.as_str()) || !entry.entry_type.is_installed_by_itself() {
            continue;
        }
        for (id, minimum) in stated_minimums(root, entry, engine) {
            let is_broken = versions.get(id.as_str()).is_some_and(|(before, now)| {
                satisfies(Some(&minimum), before) && !satisfies(Some(&minimum), now)
            });
            if is_broken {
                broken.entry(id).or_default().insert(entry.id.clone());
            }
        }
        for provider in providers_lost(entry, engine, after) {
            if versions.contains_key(provider.as_str()) {
                broken.entry(provider).or_default().insert(entry.id.clone());
            }
        }
    }
    for (id, dependents) in broken {
        findings.block(PlanBlocker::BreaksDependents {
            id,
            dependents: dependents.into_iter().collect(),
        });
    }
}

fn operations_of<'a>(
    entry: &'a CatalogEntry,
    catalog: &'a EngineCatalog,
) -> impl Iterator<Item = &'a OperationContract> {
    let operations = &catalog.operations;
    operations
        .operations()
        .filter(move |operation| operations.owner_of(&operation.id) == Some(entry.id.as_str()))
}

/// Versions minimales qu'un élément du moteur exige d'autres éléments.
fn stated_minimums(
    root: &Path,
    entry: &CatalogEntry,
    engine: &EngineCatalog,
) -> Vec<(String, String)> {
    let dependencies = load_manifest(root, &entry.manifest_path)
        .map(|manifest| manifest.dependencies)
        .unwrap_or_default();
    let of_tools = operations_of(entry, engine)
        .filter_map(|operation| engine.operations.raw(&operation.id))
        .flat_map(stated_tools)
        .filter_map(|(id, minimum)| Some((id.to_owned(), minimum?.to_owned())));
    dependencies
        .into_iter()
        .filter_map(|dependency| Some((dependency.id, dependency.version_range?)))
        .chain(of_tools)
        .collect()
}

/// Fournisseurs qui n'exposeraient plus ce qu'un script du moteur lit.
fn providers_lost(
    entry: &CatalogEntry,
    before: &EngineCatalog,
    after: &EngineCatalog,
) -> BTreeSet<String> {
    let operations = &before.operations;
    operations_of(entry, before)
        .flat_map(requirements_of)
        .filter(|requirement| {
            operations.is_requirement_met(requirement)
                && !after.operations.is_requirement_met(requirement)
        })
        .map(|requirement| requirement.provider.clone())
        .collect()
}

/// Blocs et opérations retirés : accord pour les flux et les modèles, refus pour les tâches.
fn check_graphs(context: &PlanContext, after: &EngineCatalog, findings: &mut Findings) {
    let before = &context.engine;
    let mut flows: BTreeMap<Lost, BTreeSet<String>> = BTreeMap::new();
    for (name, graph) in context.usage.named_graphs() {
        for lost in lost_by(graph, before, after) {
            flows.entry(lost).or_default().insert(name.to_owned());
        }
    }
    let mut jobs: BTreeMap<Lost, BTreeSet<String>> = BTreeMap::new();
    for (sources, graph) in context.usage.tasks() {
        for lost in graph.map_or_else(BTreeSet::new, |graph| lost_by(graph, before, after)) {
            jobs.entry(lost)
                .or_default()
                .extend(sources.iter().cloned());
        }
    }
    for ((id, removed), jobs) in jobs {
        findings.block(PlanBlocker::InUseByJobs {
            id,
            removed,
            jobs: jobs.into_iter().collect(),
        });
    }
    for ((id, removed), flows) in flows {
        findings.ask(PlanConsent::Removes {
            id,
            removed,
            flows: flows.into_iter().collect(),
        });
    }
}

/// Ce que ce graphe trouvait dans le moteur et n'y trouverait plus.
fn lost_by(graph: &FlowGraph, before: &EngineCatalog, after: &EngineCatalog) -> BTreeSet<Lost> {
    let mut lost = BTreeSet::new();
    let owner_of = |operation: &OperationContract| {
        before
            .operations
            .owner_of(&operation.id)
            .unwrap_or_default()
            .to_owned()
    };
    for node in &graph.nodes {
        let block = find_owned(&before.blocks, &node.kind);
        let operation = before.operations.resolve_node_operation(node);
        // Un nœud qui nomme son opération s'exécute encore sans fiche de bloc.
        let is_lost = match operation {
            Some(_) => after.operations.resolve_node_operation(node).is_none(),
            None => block.is_some() && find_owned(&after.blocks, &node.kind).is_none(),
        };
        if is_lost {
            let owner = operation.map(owner_of).or_else(|| block?.owner.clone());
            lost.insert((owner.unwrap_or_default(), node.kind.as_str().to_owned()));
        }
        if let Some(operation) = before.operations.library_script_operation(node)
            && after.operations.library_script_operation(node).is_none()
        {
            lost.insert((owner_of(operation), operation.id.clone()));
        }
    }
    lost
}

/// Une mise à jour ne retire ni la dernière sonde primaire ni la dernière opération par défaut.
fn check_base(
    context: &PlanContext,
    updates: &[&Change],
    after: &EngineCatalog,
    findings: &mut Findings,
) {
    let before = &context.engine;
    let default =
        |catalog: &EngineCatalog| catalog.operations.default_fallback_operation().is_some();
    let losses = [
        (
            before.has_primary_probe() && !after.has_primary_probe(),
            "dernière sonde primaire",
        ),
        (
            default(before) && !default(after),
            "dernière opération par défaut",
        ),
    ];
    for (_, role) in losses.into_iter().filter(|(is_lost, _)| *is_lost) {
        for change in updates {
            findings.block(PlanBlocker::InvalidItem {
                id: change.content.entry.id.clone(),
                message: format!("cette version retire la {role} du moteur"),
            });
        }
    }
}

/// Variables exposées retirées ou de version majeure changée, dont un flux a besoin.
fn check_exposures(
    context: &PlanContext,
    updates: &[&Change],
    after: &EngineCatalog,
    findings: &mut Findings,
) {
    let reduced: Vec<(&str, &str)> = updates
        .iter()
        .flat_map(|change| reduced_namespaces(change))
        .collect();
    if reduced.is_empty() {
        return;
    }
    let before = &context.engine.operations;
    let flows: BTreeSet<String> = context
        .usage
        .named_graphs()
        .filter(|(_, graph)| {
            let known = check_references(graph, before);
            check_references(graph, &after.operations)
                .iter()
                .any(|problem| !known.contains(problem))
        })
        .map(|(name, _)| name.to_owned())
        .collect();
    if flows.is_empty() {
        return;
    }
    for (id, namespace) in reduced {
        findings.ask(PlanConsent::Removes {
            id: id.to_owned(),
            removed: namespace.to_owned(),
            flows: flows.iter().cloned().collect(),
        });
    }
}

/// Espaces de l'ancienne copie dont une variable disparaît.
fn reduced_namespaces(change: &Change) -> Vec<(&str, &str)> {
    let Some((_, declared)) = change
        .previous
        .as_ref()
        .and_then(|previous| previous.copy.as_ref())
    else {
        return Vec::new();
    };
    let is_kept = |old: &ExposedVariables| {
        change.declared.exposures().any(|new| {
            new.namespace == old.namespace
                && new.contract_major() == old.contract_major()
                && old.variables.iter().all(|variable| {
                    new.variables
                        .iter()
                        .any(|known| known.name == variable.name)
                })
        })
    };
    declared
        .exposures()
        .filter(|old| !is_kept(old))
        .map(|old| (change.content.entry.id.as_str(), old.namespace.as_str()))
        .collect()
}
