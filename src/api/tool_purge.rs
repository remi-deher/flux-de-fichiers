//! Purge des versions d'outils que le catalogue n'attend plus, dans le cache du central.

use super::AppState;
use crate::{plugins, repository};
use std::time::Duration;
use tokio::time::Instant;

/// Attente avant de retenter une purge reportée par une tâche en cours.
const PURGE_RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Purge dès qu'aucune tâche intégrée ne l'empêche, sans dépasser `deadline`.
pub(super) async fn prune_before(state: &AppState, deadline: Instant) {
    while !prune(state).await {
        let retry = Instant::now() + PURGE_RETRY_INTERVAL;
        if retry >= deadline {
            return;
        }
        tokio::time::sleep_until(retry).await;
    }
}

/// Purge le cache du central, sauf si une tâche du worker intégré peut encore y prendre ses outils, ou si
/// les éléments installés ne disent pas encore lesquels servent. Rend `false` si la purge est reportée.
async fn prune(state: &AppState) -> bool {
    let engine = match plugins::engine_sources(&state.pool, &state.config).await {
        Ok(engine) => engine,
        Err(error) => {
            tracing::warn!(%error, "purge des outils reportée : catalogue illisible");
            return false;
        }
    };
    if !engine.is_complete() {
        tracing::debug!("pas de purge des outils : éléments installés à reprendre ou à réparer");
        return true;
    }
    let cache = state.tool_cache();
    let keep: Vec<(String, String)> = state
        .catalog_cache
        .tools_status(&engine.roots, &cache)
        .await
        .into_iter()
        .map(|tool| (tool.id, tool.version))
        .collect();
    match repository::integrated_lease_active(&state.pool).await {
        Ok(false) => {}
        Ok(true) => {
            tracing::debug!("purge des outils reportée : le worker intégré exécute une tâche");
            return false;
        }
        Err(error) => {
            tracing::warn!(%error, "purge des outils reportée : baux illisibles");
            return false;
        }
    }
    if let Ok(pruned) = cache.prune_unused(&keep)
        && pruned > 0
    {
        tracing::info!(
            pruned,
            "anciennes versions d'outils purgées du cache central"
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::transfers::tests::setup;
    use std::{path::Path, sync::Arc};

    const STALE: &str = "demo/1.0.0";
    const CURRENT: &str = "demo/2.0.0";

    /// Remplace le catalogue du central par celui du dossier donné.
    fn with_catalog(mut state: AppState, plugins_dir: &Path) -> AppState {
        let mut config = (*state.config).clone();
        config.plugins_dir = Some(plugins_dir.to_path_buf());
        state.config = Arc::new(config);
        state
    }

    #[tokio::test]
    async fn tools_are_purged_only_with_a_catalog_and_without_a_local_task() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, _, lease, _) = setup(dir.path()).await?;
        let tools = state.config.tools_cache_dir.clone();
        for version in [STALE, CURRENT] {
            std::fs::create_dir_all(tools.join(version))?;
        }

        let catalog = dir.path().join("catalogue");
        std::fs::create_dir_all(&catalog)?;
        std::fs::write(
            catalog.join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"demo","type":"app","version":"2.0.0","name":"Démo","manifestPath":"apps/demo/manifest.json"}
            ]}"#,
        )?;
        let empty = state.clone();
        let state = with_catalog(state, &catalog);

        // Reprise des dépôts en cours : d'autres outils peuvent servir.
        assert!(prune(&state).await);
        assert!(tools.join(STALE).is_dir());
        repository::set_pending_store_repositories(&state.pool, &[]).await?;

        // Aucun catalogue lisible : rien ne dit quels outils servent encore.
        assert!(prune(&empty).await);
        assert!(tools.join(STALE).is_dir());

        // Le titulaire du bail en cours devient le worker intégré.
        repository::set_serves_central(&state.pool, lease.worker_id, true).await?;
        assert!(!prune(&state).await);
        // Une échéance trop proche n'attend pas un nouvel essai.
        prune_before(&state, Instant::now()).await;
        assert!(tools.join(STALE).is_dir());

        repository::set_serves_central(&state.pool, lease.worker_id, false).await?;
        assert!(prune(&state).await);
        assert!(!tools.join(STALE).exists());
        assert!(tools.join(CURRENT).is_dir());
        Ok(())
    }
}
