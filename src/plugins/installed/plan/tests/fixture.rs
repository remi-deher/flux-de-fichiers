//! Central de test : dépôts écrits sur disque, éléments installés pour de vrai.

use super::{super::PlanContext, OFFICIAL, item::Item};
use crate::{
    api::AppState,
    error::{AppError, AppResult},
    plugins::{
        CatalogItemType, PreparedPlan,
        installed::{freeze_item, lock_installations, read_item, resume_store},
        plan_installation, repository_dir,
    },
    repository::{self, InstallationRecord},
};
use forgearr_shared::{
    flows::UpdateFlowRequest,
    plugins::store::{ItemOrigin, ItemRef},
};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) struct Central {
    pub state: AppState,
    _dir: tempfile::TempDir,
}

pub(super) fn target(repository: &str, id: &str) -> ItemRef {
    ItemRef {
        repository_id: repository.to_owned(),
        id: id.to_owned(),
    }
}

impl Central {
    pub async fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let state = crate::api::transfers::tests::setup(dir.path()).await?.0;
        repository::set_pending_store_repositories(&state.pool, &[]).await?;
        Ok(Self { state, _dir: dir })
    }

    /// Central qui a ces éléments, tous installés depuis le dépôt officiel.
    pub async fn with_installed(items: &[Item]) -> anyhow::Result<Self> {
        let central = Self::new().await?;
        central.publish(OFFICIAL, items).await?;
        for item in items {
            central.install(OFFICIAL, &item.id).await?;
        }
        Ok(central)
    }

    fn root(&self, repository: &str) -> anyhow::Result<PathBuf> {
        repository_dir(&self.state.config.plugin_repos_dir(), repository)
            .ok_or_else(|| anyhow::anyhow!("{repository} : dossier refusé"))
    }

    /// Remplace tout le contenu d'un dépôt, ajouté s'il ne l'est pas encore.
    pub async fn publish(&self, repository: &str, items: &[Item]) -> anyhow::Result<()> {
        let state = &self.state;
        match repository::get_plugin_repository(&state.pool, repository).await {
            Ok(_) => {}
            Err(AppError::NotFound) => {
                let url = format!("https://{repository}.example/index.json");
                repository::add_plugin_repository(
                    &state.pool,
                    &state.changes,
                    repository,
                    repository,
                    &url,
                )
                .await?;
            }
            Err(error) => return Err(error.into()),
        }
        let root = self.root(repository)?;
        if root.exists() {
            std::fs::remove_dir_all(&root)?;
        }
        std::fs::create_dir_all(&root)?;
        let mut entries: Vec<Value> = Vec::new();
        for item in items {
            entries.extend(item.write(&root)?);
        }
        let index = json!({"schemaVersion": "1.0.0", "entries": entries});
        std::fs::write(root.join("index.json"), index.to_string())?;
        state.catalog_cache.invalidate().await;
        Ok(())
    }

    pub async fn origin(&self, repository: &str) -> anyhow::Result<ItemOrigin> {
        let repository = repository::get_plugin_repository(&self.state.pool, repository).await?;
        Ok(ItemOrigin {
            repository_id: repository.id,
            repository_url: repository.index_url,
            repository_name: repository.name,
        })
    }

    pub async fn install(&self, repository: &str, id: &str) -> anyhow::Result<()> {
        let state = &self.state;
        let origin = self.origin(repository).await?;
        let content = read_item(&self.root(repository)?, id)?;
        let is_script = content.entry.entry_type == CatalogItemType::Script;
        let record = InstallationRecord {
            item_id: id.to_owned(),
            kind: content.entry.entry_type,
            version: content.entry.version.clone(),
            content_sha256: content.content_sha256.clone(),
            origin: origin.clone(),
            copy_id: is_script.then(|| id.to_owned()),
            copy_pending: is_script,
            serves_every_task: false,
        };
        {
            let lock = lock_installations().await;
            freeze_item(&lock, &state.config.installed_dir(), content, &origin).await?;
            let mut conn = state.pool.acquire().await?;
            repository::save_installation(&mut conn, &record).await?;
        }
        // Écrit la copie modifiable d'un script.
        resume_store(&state.pool, &state.config, &state.catalog_cache, None).await?;
        state.catalog_cache.invalidate().await;
        Ok(())
    }

    /// Dossier figé d'un élément installé.
    pub async fn frozen(&self, id: &str) -> anyhow::Result<PathBuf> {
        let installed = repository::list_installations(&self.state.pool).await?;
        let record = installed.iter().map(|item| &item.record);
        record
            .into_iter()
            .find(|record| record.item_id == id)
            .and_then(|record| {
                crate::plugins::installed_root(&self.state.config.installed_dir(), record)
            })
            .ok_or_else(|| anyhow::anyhow!("{id} n'est pas installé"))
    }

    /// Flux d'un nœud par type de bloc.
    pub async fn save_flow(&self, id: &str, kinds: &[&str]) -> anyhow::Result<()> {
        let nodes: Vec<Value> = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| json!({"id": format!("n{index}"), "type": kind, "name": kind}))
            .collect();
        let flow = UpdateFlowRequest {
            name: id.to_owned(),
            description: None,
            file_mask: "*".to_owned(),
            is_default: false,
            definition: json!({"nodes": nodes, "connections": []}).to_string(),
        };
        repository::save_flow(&self.state.pool, id, &flow).await?;
        Ok(())
    }

    /// Tâche en attente de ce flux.
    pub async fn queue(&self, source: &str, flow: &str) -> anyhow::Result<()> {
        let sha = "ab".repeat(32);
        repository::insert_job_with_options(
            &self.state.pool,
            source,
            None,
            &sha,
            1,
            None,
            Some(flow),
        )
        .await?;
        Ok(())
    }

    /// Réglages d'un plugin, saisis pour cette origine.
    pub async fn save_settings(&self, id: &str, repository: &str) -> anyhow::Result<()> {
        let state = &self.state;
        let settings = r#"{"url":"http://nas"}"#;
        repository::set_plugin_settings(&state.pool, &state.changes, id, settings).await?;
        let origin = self.origin(repository).await?;
        let mut conn = state.pool.acquire().await?;
        repository::set_settings_origin(&mut conn, id, &origin).await?;
        Ok(())
    }

    pub async fn context(&self) -> AppResult<PlanContext> {
        let state = &self.state;
        PlanContext::read(&state.pool, &state.config, &state.catalog_cache, true).await
    }

    pub async fn plan_as(
        &self,
        targets: &[(&str, &str)],
        may_manage_flows: bool,
    ) -> AppResult<PreparedPlan> {
        let state = &self.state;
        let targets: Vec<ItemRef> = targets
            .iter()
            .map(|(repository, id)| target(repository, id))
            .collect();
        plan_installation(
            &state.pool,
            &state.config,
            &state.catalog_cache,
            &targets,
            may_manage_flows,
        )
        .await
    }

    pub async fn plan(&self, targets: &[(&str, &str)]) -> AppResult<PreparedPlan> {
        self.plan_as(targets, true).await
    }
}
