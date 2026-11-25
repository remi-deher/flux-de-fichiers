//! Noms uniques dans le moteur (ADR 0029 §10) : identifiants, types de blocs, alias et opérations. Ce que l'élément
//! installé revendiquait déjà n'est plus contesté.

use super::{
    after::engine_changes,
    context::PlanContext,
    declared::Declared,
    findings::Findings,
    walk::{Change, Selected},
};
use crate::{flows::contract::library_operation_id, plugins::CatalogItemType};
use forgearr_shared::plugins::{
    CatalogEntry,
    store::plan::{ClaimKind, PlanBlocker, PlanWarning},
};
use std::collections::{BTreeSet, HashMap, HashSet};

type Claim = (ClaimKind, String);

pub(super) fn check_claims(context: &PlanContext, selected: &[Selected], findings: &mut Findings) {
    let changed: HashSet<&str> = engine_changes(selected)
        .map(|change| change.content.entry.id.as_str())
        .collect();
    // Ce que les autres éléments du plan prennent.
    let mut taken: HashMap<Claim, &str> = HashMap::new();
    for change in engine_changes(selected) {
        let entry = &change.content.entry;
        let before = known_copy(change);
        let inherited: Vec<Claim> = before
            .map(|(entry, declared)| claims_of(entry, declared))
            .unwrap_or_default();
        let mut contested: BTreeSet<String> = BTreeSet::new();
        for claim in claims_of(entry, &change.declared) {
            let in_plan = taken.get(&claim).copied().filter(|id| *id != entry.id);
            let holder = in_plan.map(str::to_owned).or_else(|| {
                holder_in_engine(context, &claim)
                    .filter(|holder| *holder != entry.id && !changed.contains(holder.as_str()))
            });
            let is_inherited = inherited.contains(&claim);
            taken.entry(claim.clone()).or_insert(&entry.id);
            let (kind, value) = claim;
            // Un bloc porte souvent le même nom dans l'index : une seule ligne suffit.
            if let Some(holder) = holder
                && !is_inherited
                && contested.insert(value.clone())
            {
                findings.block(PlanBlocker::ClaimConflict {
                    id: entry.id.clone(),
                    claim: kind,
                    value,
                    holder,
                });
            }
        }
        check_providers(change, findings);
        warn_of_shared_namespaces(context, change, findings);
    }
}

/// Entrée et déclarations de la copie que l'élément remplace.
fn known_copy(change: &Change) -> Option<(&CatalogEntry, &Declared)> {
    let (content, declared) = change.previous.as_ref()?.copy.as_ref()?;
    Some((&content.entry, declared))
}

fn claims_of(entry: &CatalogEntry, declared: &Declared) -> Vec<Claim> {
    let kinds = declared.blocks.iter().flat_map(|block| {
        std::iter::once(&block.kind)
            .chain(&block.aliases)
            .map(|kind| (ClaimKind::BlockKind, kind.clone()))
    });
    let operations = declared
        .operations
        .iter()
        .map(|operation| (ClaimKind::Operation, operation.contract.id.clone()));
    // Nom sous lequel le moteur cherche un script de la librairie.
    let library = (entry.entry_type == CatalogItemType::Script)
        .then(|| (ClaimKind::Operation, library_operation_id(&entry.id)));
    let ids = declared
        .entry_ids
        .iter()
        .map(|id| (ClaimKind::EntryId, id.clone()));
    kinds.chain(operations).chain(library).chain(ids).collect()
}

fn holder_in_engine(context: &PlanContext, (kind, value): &Claim) -> Option<String> {
    let engine = &context.engine;
    match kind {
        ClaimKind::EntryId => engine.entry(value).map(|entry| match entry.entry_type {
            // Un pack de la collection masquerait l'élément de même identifiant.
            CatalogItemType::Pack => format!("pack {}", entry.id),
            _ => entry.owner.clone().unwrap_or_else(|| entry.id.clone()),
        }),
        ClaimKind::BlockKind => engine
            .blocks
            .iter()
            .find(|block| block.kind == *value || block.aliases.contains(value))
            .and_then(|block| block.owner.clone()),
        ClaimKind::Operation => {
            let owner = engine.operations.owner_of(value).map(str::to_owned);
            owner.or_else(|| {
                engine
                    .entries
                    .iter()
                    .map(|(_, entry)| entry)
                    .filter(|entry| entry.entry_type == CatalogItemType::Script)
                    .find(|entry| library_operation_id(&entry.id) == *value)
                    .map(|entry| entry.id.clone())
            })
        }
    }
}

/// Une opération ne parle qu'au nom de l'élément qui la porte.
fn check_providers(change: &Change, findings: &mut Findings) {
    let id = &change.content.entry.id;
    let before = known_copy(change);
    for operation in &change.declared.operations {
        let contract = &operation.contract;
        let was_declared = before.is_some_and(|(_, declared)| {
            declared
                .operation(&contract.id)
                .is_some_and(|known| known.contract.provider_id == contract.provider_id)
        });
        if contract.provider_id != *id && !was_declared {
            findings.block(PlanBlocker::ForeignProvider {
                id: id.clone(),
                operation: contract.id.clone(),
                provider: contract.provider_id.clone(),
            });
        }
    }
}

/// Espaces de variables qu'un autre élément expose déjà : permis (ADR 0025).
fn warn_of_shared_namespaces(context: &PlanContext, change: &Change, findings: &mut Findings) {
    let id = &change.content.entry.id;
    let known = known_copy(change)
        .map(|(_, declared)| namespaces(declared))
        .unwrap_or_default();
    let claims = context.engine.operations.exposure_claims();
    for namespace in namespaces(&change.declared).difference(&known) {
        let holder = claims
            .iter()
            .find(|(provider, exposed)| provider != id && exposed.namespace == *namespace);
        if let Some((holder, _)) = holder {
            findings.warn(PlanWarning::SharedNamespace {
                id: id.clone(),
                namespace: (*namespace).to_owned(),
                holder: (*holder).to_owned(),
            });
        }
    }
}

fn namespaces(declared: &Declared) -> BTreeSet<&str> {
    declared
        .exposures()
        .map(|exposed| exposed.namespace.as_str())
        .collect()
}
