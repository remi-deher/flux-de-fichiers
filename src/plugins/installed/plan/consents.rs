//! Ce que l'utilisateur doit accepter explicitement : réglages d'un autre dépôt effacés, réglages lus par un autre
//! plugin, sonde ou opération par défaut remplacée.

use super::{context::PlanContext, findings::Findings, walk::Selected};
use crate::{
    flows::{OperationContract, OperationRegistry, OperationRuntime},
    plugins::installed::EngineCatalog,
};
use forgearr_shared::plugins::store::plan::{PlanAction, PlanConsent};
use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
};

/// Nom d'un fichier qu'aucune sonde ne reconnaît à son extension.
const PLAIN_SAMPLE: &str = "fichier";

pub(super) fn ask_consents(
    context: &PlanContext,
    selected: &[Selected],
    after: &EngineCatalog,
    findings: &mut Findings,
) {
    for item in selected {
        let Some(change) = &item.change else {
            continue;
        };
        if change.action == PlanAction::Install && discards_settings(context, item) {
            findings.ask(PlanConsent::DiscardSettings {
                id: item.id.clone(),
            });
        }
        // Le même contenu a déjà été accepté.
        if change.action != PlanAction::Repair {
            for operation in &change.declared.operations {
                if let OperationRuntime::Http(http) = &operation.contract.runtime
                    && let Some(plugin) = &http.settings_plugin_id
                    && *plugin != item.id
                {
                    findings.ask(PlanConsent::ReadsSettings {
                        id: item.id.clone(),
                        plugin: plugin.clone(),
                    });
                }
            }
        }
    }
    let changed: HashSet<&str> = selected
        .iter()
        .filter(|item| item.change.is_some())
        .map(|item| item.id.as_str())
        .collect();
    let (before, after) = (&context.engine.operations, &after.operations);
    for (id, current) in replaced_probes(before, after) {
        if changed.contains(id.as_str()) {
            findings.ask(PlanConsent::ReplacesPrimaryProbe { id, current });
        }
    }
    let default = replacement(before, after, OperationRegistry::default_fallback_operation);
    if let Some((id, current)) = default.filter(|(id, _)| changed.contains(id.as_str())) {
        findings.ask(PlanConsent::ReplacesDefaultOperation { id, current });
    }
}

/// Les réglages conservés viennent-ils d'une autre origine ?
pub(super) fn discards_settings(context: &PlanContext, item: &Selected) -> bool {
    context
        .settings
        .get(&item.id)
        .is_some_and(|saved| !saved.belong_to(&item.origin))
}

/// Nouveau porteur et porteur actuel de l'opération choisie, s'ils diffèrent.
fn replacement(
    before: &OperationRegistry,
    after: &OperationRegistry,
    choose: impl Fn(&OperationRegistry) -> Option<&OperationContract>,
) -> Option<(String, String)> {
    let holder = |registry: &OperationRegistry| {
        choose(registry)
            .and_then(|operation| registry.owner_of(&operation.id))
            .map(str::to_owned)
    };
    let (current, new) = (holder(before)?, holder(after)?);
    (new != current).then_some((new, current))
}

/// Sondes primaires remplacées, toutes extensions confondues : une sonde qui nomme une extension passe devant celles
/// qui les prennent toutes.
fn replaced_probes(
    before: &OperationRegistry,
    after: &OperationRegistry,
) -> BTreeSet<(String, String)> {
    let samples: BTreeSet<PathBuf> = [before, after]
        .into_iter()
        .flat_map(OperationRegistry::operations)
        .filter_map(|operation| match &operation.runtime {
            OperationRuntime::Binary(binary) if OperationRegistry::is_primary_probe(operation) => {
                Some(&binary.probe_match.extensions)
            }
            _ => None,
        })
        .flatten()
        .map(|extension| {
            let extension = extension.trim().trim_start_matches('.').to_lowercase();
            PathBuf::from(format!("{PLAIN_SAMPLE}.{extension}"))
        })
        .chain([PathBuf::from(PLAIN_SAMPLE)])
        .collect();
    samples
        .iter()
        .filter_map(|sample| {
            replacement(before, after, |registry| {
                registry.primary_probe_for_file(sample)
            })
        })
        .chain(replacement(before, after, OperationRegistry::primary_probe))
        .collect()
}
