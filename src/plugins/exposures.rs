//! Variables exposées par les plugins du catalogue, avec l'état de chaque espace de noms (ADR 0025).

use crate::flows::OperationRegistry;
use forgearr_shared::exposed_variables::{ExposedNamespaceView, ExposedVariables};

/// Exposition déclarée : le plugin fournisseur et son contrat.
type Claim<'a> = (&'a str, &'a ExposedVariables);

/// Expositions des opérations chargées. À espace de noms égal, la première (par identifiant
/// d'opération) est celle que le moteur lit : les autres sont signalées comme contestées.
pub fn exposure_views(registry: &OperationRegistry) -> Vec<ExposedNamespaceView> {
    let claims = registry.exposure_claims();
    claims
        .iter()
        .enumerate()
        .map(|(index, (provider, exposes))| {
            let same_namespace = |other: &Claim<'_>| other.1.namespace == exposes.namespace;
            let active = !claims[..index].iter().any(same_namespace);
            let mut conflicts_with: Vec<String> = claims
                .iter()
                .filter(|other| same_namespace(other) && other.0 != *provider)
                .map(|other| other.0.to_owned())
                .collect();
            conflicts_with.sort();
            conflicts_with.dedup();
            ExposedNamespaceView {
                provider: (*provider).to_owned(),
                exposes: (*exposes).clone(),
                active,
                conflicts_with,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn operation(op_id: &str, provider: &str, namespace: &str) -> serde_json::Value {
        json!({
            "schemaVersion": "1.1.0", "id": op_id, "version": "1.0.0", "name": "Op",
            "providerId": provider, "providerKind": "plugin", "capability": "probe", "status": "validated",
            "runtime": {"kind": "primitive", "primitive": "entry"},
            "exposes": {"namespace": namespace, "contractVersion": "1.0.0",
                        "variables": [{"name": "video.codec", "path": "video.codec"}]}
        })
    }

    fn registry(operations: &[serde_json::Value]) -> anyhow::Result<OperationRegistry> {
        let mut registry = OperationRegistry::new();
        for raw in operations {
            registry.register_json(raw.clone())?;
        }
        Ok(registry)
    }

    #[test]
    fn a_plugin_alone_in_its_namespace_is_active_without_conflicts() -> anyhow::Result<()> {
        let views = exposure_views(&registry(&[operation(
            "operation.a",
            "alpha",
            "mediainfo",
        )])?);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].provider, "alpha");
        assert!(views[0].active);
        assert!(views[0].conflicts_with.is_empty());
        assert_eq!(views[0].exposes.namespace, "mediainfo");
        Ok(())
    }

    #[test]
    fn two_plugins_in_one_namespace_conflict_and_only_the_first_is_active() -> anyhow::Result<()> {
        let views = exposure_views(&registry(&[
            operation("operation.b", "beta", "mediainfo"),
            operation("operation.a", "alpha", "mediainfo"),
            operation("operation.c", "gamma", "autre"),
        ])?);
        let by_provider = |name: &str| views.iter().find(|view| view.provider == name);
        let alpha = by_provider("alpha").ok_or_else(|| anyhow::anyhow!("alpha absent"))?;
        let beta = by_provider("beta").ok_or_else(|| anyhow::anyhow!("beta absent"))?;
        let gamma = by_provider("gamma").ok_or_else(|| anyhow::anyhow!("gamma absent"))?;
        assert!(alpha.active && !beta.active && gamma.active);
        assert_eq!(alpha.conflicts_with, ["beta"]);
        assert_eq!(beta.conflicts_with, ["alpha"]);
        assert!(gamma.conflicts_with.is_empty());
        Ok(())
    }

    #[test]
    fn a_second_operation_of_the_same_plugin_is_shadowed_without_naming_a_conflict()
    -> anyhow::Result<()> {
        let views = exposure_views(&registry(&[
            operation("operation.a1", "alpha", "mediainfo"),
            operation("operation.a2", "alpha", "mediainfo"),
        ])?);
        assert_eq!(views.iter().filter(|view| view.active).count(), 1);
        assert!(views.iter().all(|view| view.conflicts_with.is_empty()));
        Ok(())
    }

    #[test]
    fn no_exposure_gives_no_view() -> anyhow::Result<()> {
        assert!(exposure_views(&registry(&[])?).is_empty());
        Ok(())
    }
}
