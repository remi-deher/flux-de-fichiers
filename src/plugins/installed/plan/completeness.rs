//! Fichiers qu'une source n'a pas, scripts modifiés et variables qu'aucun élément n'expose.

use super::{
    after::engine_changes, context::PlanContext, findings::Findings, states::blocking_files,
    walk::Selected,
};
use crate::{
    error::AppResult,
    plugins::installed::{
        EngineCatalog,
        script_copy::{ScriptCopy, compare_script_copy},
    },
};
use forgearr_shared::{
    exposed_variables::ScriptRequirement,
    plugins::{
        store::plan::{PlanBlocker, PlanConsent, PlanWarning},
        vocab::CatalogItemType,
    },
};
use std::path::Path;

/// Ce que l'élément aurait en moins de ce qu'il déclare.
pub(super) fn check_files(selected: &[Selected], findings: &mut Findings) {
    for change in selected.iter().filter_map(|item| item.change.as_ref()) {
        let (content, declared) = (&change.content, &change.declared);
        let id = &content.entry.id;
        let before = change
            .previous
            .as_ref()
            .and_then(|previous| previous.copy.as_ref());
        let was_without = |path: &String| {
            before.is_some_and(|(content, declared)| {
                content.missing.contains(path) || declared.unreadable.contains(path)
            })
        };
        let without: Vec<String> = content
            .missing_operations
            .iter()
            .chain(&declared.unreadable)
            .filter(|path| !was_without(path))
            .cloned()
            .collect();
        let mut missing = blocking_files(content, before.map(|(content, _)| content));
        let has_nothing_left = !without.is_empty() && declared.operations.is_empty();
        if has_nothing_left {
            missing.extend(without.iter().cloned());
        }
        if !missing.is_empty() {
            findings.block(PlanBlocker::Incomplete {
                id: id.clone(),
                files: missing,
            });
        } else if !without.is_empty() {
            findings.ask(PlanConsent::PartialInstall {
                id: id.clone(),
                files: without,
            });
        }
        if !content.skipped.is_empty() {
            findings.warn(PlanWarning::SkippedFiles {
                id: id.clone(),
                files: content.skipped.clone(),
            });
        }
    }
}

/// Un script modifié n'est pas remplacé.
pub(super) fn check_script_copies(
    context: &PlanContext,
    selected: &[Selected],
    findings: &mut Findings,
) -> AppResult<()> {
    let scripts = selected.iter().filter(|item| {
        item.kind == CatalogItemType::Script
            && item
                .change
                .as_ref()
                .is_some_and(|change| change.previous.is_some())
    });
    for script in scripts {
        let frozen = context
            .record(&script.id)
            .and_then(|record| context.frozen_root(record));
        // Sans copie figée, rien ne dit que la copie est restée la même.
        let frozen = frozen.as_deref().unwrap_or(Path::new(""));
        if compare_script_copy(frozen, context.local_dir(), &script.id)? == ScriptCopy::Modified {
            findings.block(PlanBlocker::ModifiedCopy {
                id: script.id.clone(),
            });
        }
    }
    Ok(())
}

/// Un fournisseur présent doit exposer ce qu'un script lit ; absent, il est déjà signalé. Ce que l'élément installé
/// exigeait déjà sans l'avoir n'est pas une raison de refuser sa mise à jour.
pub(super) fn check_requirements(
    context: &PlanContext,
    selected: &[Selected],
    after: &EngineCatalog,
    findings: &mut Findings,
) {
    let before = &context.engine.operations;
    for change in engine_changes(selected) {
        let installed = change
            .previous
            .as_ref()
            .and_then(|previous| previous.copy.as_ref());
        let was_unmet = |requirement: &ScriptRequirement| {
            !before.is_requirement_met(requirement)
                && installed.is_some_and(|(_, declared)| {
                    declared.requirements().any(|known| known == requirement)
                })
        };
        let unmet = change.declared.requirements().filter(|requirement| {
            !after.operations.is_requirement_met(requirement)
                && after.entry(&requirement.provider).is_some()
                && !was_unmet(requirement)
        });
        for requirement in unmet {
            findings.block(PlanBlocker::InvalidItem {
                id: change.content.entry.id.clone(),
                message: format!(
                    "« {} » n'expose pas la variable « {} » (version {})",
                    requirement.provider, requirement.variable, requirement.contract_major
                ),
            });
        }
    }
}
