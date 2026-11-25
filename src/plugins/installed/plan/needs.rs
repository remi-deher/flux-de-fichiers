//! Ce qu'un élément exige d'autres éléments : dépendances déclarées, et déduites de ses opérations, de ses blocs
//! ou de ceux d'un flux.

use super::{context::PlanContext, declared::Declared};
use crate::{
    flows::{contract::library_script_id, is_unknown_block},
    plugins::installed::{EngineCatalog, usage::providers_of},
};
use forgearr_shared::{
    exposed_variables::RepositoryPointer,
    flows::{FlowGraph, FlowNode, find_owned},
    plugins::store::plan::PlanReason,
};
use serde_json::Value;
use std::collections::BTreeSet;

/// Ce que devient une exigence qu'aucune source ne satisfait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Strictness {
    /// Blocage.
    Required,
    /// Outil d'une opération : absent, il peut être dans le `PATH` ; de mauvaise version, blocage.
    Recommended,
    /// Avertissement.
    Optional,
    /// Exigence d'un élément déjà installé : rien à en dire.
    Inherited,
}

#[derive(Clone, Debug)]
pub(super) struct Need {
    pub id: String,
    pub minimum: Option<String>,
    pub strictness: Strictness,
    pub reason: PlanReason,
    pub required_by: String,
    /// Source de celui qui l'exige, cherchée d'abord.
    pub source: Option<usize>,
    /// Dépôt désigné pour la fournir.
    pub repository: Option<RepositoryPointer>,
}

/// Celui qui exige.
#[derive(Clone, Copy)]
pub(super) struct Requester<'a> {
    pub id: &'a str,
    pub source: Option<usize>,
}

impl Requester<'_> {
    pub fn needs(&self, id: impl Into<String>, strictness: Strictness) -> Need {
        Need {
            id: id.into(),
            minimum: None,
            strictness,
            reason: PlanReason::Required,
            required_by: self.id.to_owned(),
            source: self.source,
            repository: None,
        }
    }
}

/// Outils que lance une opération, avec la version minimale si elle la dit.
pub(super) fn stated_tools(operation: &Value) -> impl Iterator<Item = (&str, Option<&str>)> {
    let runtime = operation.get("runtime");
    let field = move |name: &str| runtime.and_then(|runtime| runtime.get(name));
    let path_tools = field("pathTools").and_then(Value::as_array);
    field("tool")
        .into_iter()
        .chain(path_tools.into_iter().flatten())
        .filter_map(|tool| {
            let text = |name: &str| tool.get(name).and_then(Value::as_str);
            Some((text("id")?, text("version")))
        })
}

/// Ce qu'exige un élément, hors blocs d'un flux.
pub(super) fn needs_of(
    declared: &Declared,
    requester: Requester<'_>,
    context: &PlanContext,
) -> Vec<Need> {
    let mut needs: Vec<Need> = declared
        .dependencies
        .iter()
        .map(|dependency| Need {
            minimum: dependency.version_range.clone(),
            ..requester.needs(
                &dependency.id,
                if dependency.optional {
                    Strictness::Optional
                } else {
                    Strictness::Required
                },
            )
        })
        .collect();
    for operation in &declared.operations {
        needs.extend(stated_tools(&operation.raw).map(|(tool, minimum)| Need {
            minimum: minimum.map(str::to_owned),
            ..requester.needs(tool, Strictness::Recommended)
        }));
        needs.extend(
            providers_of(&operation.contract).map(|(provider, repository)| Need {
                repository: repository.cloned(),
                ..requester.needs(provider, Strictness::Required)
            }),
        );
    }
    needs.extend(
        declared
            .probe_tools
            .iter()
            .map(|tool| requester.needs(tool, Strictness::Recommended)),
    );
    // Bloc qui lance l'opération d'un autre élément.
    let borrowed = declared
        .blocks
        .iter()
        .filter_map(|block| block.operation_id.as_deref())
        .filter(|operation| declared.operation(operation).is_none());
    needs.extend(borrowed.filter_map(|operation| {
        let owner = context.find(requester.source, |catalog| {
            catalog.operations.owner_of(operation).map(str::to_owned)
        })?;
        Some(requester.needs(owner, Strictness::Required))
    }));
    needs.retain(|need| need.id != requester.id);
    needs
}

/// Éléments qui portent les blocs et les scripts d'un flux, puis ce qu'ils désignent et que rien ne porte.
pub(super) fn needs_of_graph(
    graph: &FlowGraph,
    requester: Requester<'_>,
    context: &PlanContext,
) -> (Vec<Need>, BTreeSet<String>) {
    let mut owners = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    for node in &graph.nodes {
        match context.find(requester.source, |catalog| node_owners(node, catalog)) {
            Some(found) => owners.extend(found),
            None => {
                unknown.insert(node.kind.as_str().to_owned());
            }
        }
        if let Some(script) = library_script_id(node).filter(|_| runs_library_script(node)) {
            match context.find(requester.source, |catalog| script_owner(node, catalog)) {
                Some(owner) => {
                    owners.insert(owner);
                }
                None => {
                    unknown.insert(script.to_owned());
                }
            }
        }
    }
    let needs = owners
        .into_iter()
        .map(|owner| requester.needs(owner, Strictness::Required))
        .collect();
    (needs, unknown)
}

/// Porteurs du bloc et de l'opération d'un nœud ; `None` si ce catalogue ne le connaît pas.
fn node_owners(node: &FlowNode, catalog: &EngineCatalog) -> Option<BTreeSet<String>> {
    let operations = &catalog.operations;
    if is_unknown_block(node, &catalog.blocks, operations) {
        return None;
    }
    let of_operation = operations
        .resolve_node_operation(node)
        .and_then(|operation| operations.owner_of(&operation.id));
    let of_block = find_owned(&catalog.blocks, &node.kind).and_then(|block| block.owner.as_deref());
    Some(
        of_operation
            .into_iter()
            .chain(of_block)
            .map(str::to_owned)
            .collect(),
    )
}

/// Un code saisi dans le nœud l'emporte sur le script de la librairie.
fn runs_library_script(node: &FlowNode) -> bool {
    node.config.get("mode").and_then(Value::as_str) != Some("inline")
}

fn script_owner(node: &FlowNode, catalog: &EngineCatalog) -> Option<String> {
    let operation = catalog.operations.library_script_operation(node)?;
    catalog
        .operations
        .owner_of(&operation.id)
        .map(str::to_owned)
}
