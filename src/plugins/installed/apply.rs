//! Application d'un plan (ADR 0029 §4 et §5) : il est recalculé sous le verrou, puis tous ses éléments sont
//! figés et inscrits en une transaction. Les outils cochés sont téléchargés ensuite, chaque échec étant rapporté
//! sans rien défaire.

use super::{
    InstallationGuard, PlannedChange, adopt::finish_script_copy, freeze_item, lock_installations,
    plan_installation, script_copy::check_script_copy, usage::serves_every_task,
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    plugins::{
        CatalogCache, CatalogItemType, SyncPolicy, engine_roots, ensure_tool_installed_verified,
        local_catalog_dir,
    },
    repository::{self, InstallationRecord},
    tools_cache::ToolCacheManager,
};
use forgearr_shared::{
    flows::UpdateFlowRequest,
    plugins::store::{
        ItemRef,
        plan::{
            InstallOutcome, InstallPlan, InstallRequest, InstalledRef, PlanConsent, ToolFailure,
            UseTarget,
        },
    },
};
use serde_json::Value;
use sqlx::SqlitePool;
use std::{collections::HashSet, path::Path};

/// Ce dont l'application a besoin, hors du plan.
pub struct ApplyContext<'a> {
    pub pool: &'a SqlitePool,
    pub config: &'a Config,
    pub cache: &'a CatalogCache,
    pub tools_cache: &'a ToolCacheManager,
    pub sync_policy: &'a SyncPolicy,
    pub may_manage_flows: bool,
}

/// Élément figé, prêt à être inscrit.
struct Frozen {
    record: InstallationRecord,
    installed: InstalledRef,
}

/// Installe ou met à jour ce que le plan accepté décrit.
pub async fn apply_installation(
    context: &ApplyContext<'_>,
    request: &InstallRequest,
) -> AppResult<InstallOutcome> {
    let (installed, plan) = {
        let lock = lock_installations().await;
        let prepared = plan_installation(
            context.pool,
            context.config,
            context.cache,
            &request.targets,
            context.may_manage_flows,
        )
        .await?;
        ensure_accepted(&prepared.plan, request)?;
        let installed = install_items(&lock, context, prepared.changes, &request.consents).await?;
        context.cache.invalidate().await;
        (installed, prepared.plan)
    };
    let (installed_tools, failed_tools) = download_tools(context, &plan, &request.tools).await?;
    let use_target = use_target(request.targets.first(), &installed);
    Ok(InstallOutcome {
        installed,
        installed_tools,
        failed_tools,
        use_target,
    })
}

/// Refuse un plan qui a changé, qui est bloqué, ou dont un accord manque.
fn ensure_accepted(plan: &InstallPlan, request: &InstallRequest) -> AppResult<()> {
    if plan.plan_id != request.plan_id {
        return Err(AppError::PlanOutdated);
    }
    if !plan.blockers.is_empty() {
        let reasons: Vec<String> = plan.blockers.iter().map(|b| b.message()).collect();
        return Err(AppError::InstallationBlocked(reasons.join(" | ")));
    }
    let missing: Vec<String> = plan
        .consents
        .iter()
        .filter(|consent| !request.consents.contains(consent))
        .map(PlanConsent::message)
        .collect();
    if !missing.is_empty() {
        return Err(AppError::InstallationBlocked(format!(
            "accord à donner : {}",
            missing.join(" | ")
        )));
    }
    Ok(())
}

/// Fige chaque élément, crée les flux, puis inscrit tout en une transaction.
async fn install_items(
    lock: &InstallationGuard,
    context: &ApplyContext<'_>,
    changes: Vec<PlannedChange>,
    consents: &[PlanConsent],
) -> AppResult<Vec<InstalledRef>> {
    let mut frozen = Vec::with_capacity(changes.len());
    for change in changes {
        frozen.push(freeze_change(lock, context, change).await?);
    }
    let discarded: HashSet<&str> = consents
        .iter()
        .filter_map(|consent| match consent {
            PlanConsent::DiscardSettings { id } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    let mut tx = context.pool.begin_with("BEGIN IMMEDIATE").await?;
    for item in &frozen {
        let id = item.record.item_id.as_str();
        repository::save_installation(&mut tx, &item.record).await?;
        if discarded.contains(id) {
            repository::discard_plugin_settings(&mut tx, id).await?;
        } else {
            repository::set_settings_origin(&mut tx, id, &item.record.origin).await?;
        }
    }
    tx.commit().await?;
    for item in &frozen {
        if item.record.copy_pending {
            finish_script_copy(lock, context.pool, context.config, &item.record).await?;
        }
    }
    Ok(frozen.into_iter().map(|item| item.installed).collect())
}

async fn freeze_change(
    lock: &InstallationGuard,
    context: &ApplyContext<'_>,
    change: PlannedChange,
) -> AppResult<Frozen> {
    let PlannedChange {
        origin, content, ..
    } = change;
    let entry = content.entry.clone();
    let is_script = entry.entry_type == CatalogItemType::Script;
    let installed_dir = context.config.installed_dir();
    let mut record = InstallationRecord {
        item_id: entry.id.clone(),
        kind: entry.entry_type,
        version: entry.version.clone(),
        content_sha256: content.content_sha256.clone(),
        origin: origin.clone(),
        copy_id: is_script.then(|| entry.id.clone()),
        copy_pending: is_script,
        serves_every_task: false,
    };
    let root = freeze_item(lock, &installed_dir, content, &origin).await?;
    let (item_id, local_dir) = (entry.id.clone(), local_catalog_dir(context.config));
    let frozen_root = root.clone();
    record.serves_every_task = tokio::task::spawn_blocking(move || {
        if is_script {
            check_script_copy(&installed_dir, &frozen_root, &local_dir, &item_id)?;
        }
        AppResult::Ok(serves_every_task(&frozen_root))
    })
    .await
    .map_err(|_| AppError::Internal)??;
    if entry.entry_type == CatalogItemType::Flow {
        record.copy_id = Some(create_flow(context, &root, &entry.manifest_path).await?);
    }
    let installed = InstalledRef {
        id: entry.id,
        kind: entry.entry_type,
        name: entry.name,
        version: entry.version,
        copy_id: record.copy_id.clone(),
    };
    Ok(Frozen { record, installed })
}

/// Crée un nouveau flux modifiable depuis la copie figée ; un flux installé auparavant n'est jamais touché.
async fn create_flow(
    context: &ApplyContext<'_>,
    root: &Path,
    manifest_path: &str,
) -> AppResult<String> {
    let path = root.join(manifest_path);
    let text = tokio::fs::read_to_string(&path).await?;
    let manifest: Value =
        serde_json::from_str(&text).map_err(|err| AppError::InvalidManifest(err.to_string()))?;
    let definition = manifest
        .get("definition")
        .ok_or_else(|| AppError::InvalidManifest("flux sans définition".to_owned()))?;
    let text_of = |key: &str| manifest.get(key).and_then(Value::as_str).map(str::to_owned);
    let flow_id = uuid::Uuid::new_v4().to_string();
    let mut definition = definition.to_string();
    if crate::flows::has_flow_secrets(&definition) {
        let secrets = crate::secrets::SecretBox::open(&context.config.config_dir)?;
        definition = crate::flows::seal_definition(&secrets, &flow_id, &definition, None)?;
    }
    let name = text_of("name").unwrap_or_else(|| flow_id.clone());
    let request = UpdateFlowRequest {
        name: free_flow_name(context.pool, &name).await?,
        description: text_of("description"),
        file_mask: text_of("fileMask").unwrap_or_else(|| "*".to_owned()),
        is_default: false,
        definition,
    };
    repository::save_flow(context.pool, &flow_id, &request).await?;
    Ok(flow_id)
}

/// Nom libre parmi les flux : « Nom », sinon « Nom (2) », « Nom (3) »…
async fn free_flow_name(pool: &SqlitePool, name: &str) -> AppResult<String> {
    let taken: HashSet<String> = repository::list_flows(pool)
        .await?
        .into_iter()
        .map(|flow| flow.name)
        .collect();
    Ok(first_free_name(name, &taken))
}

fn first_free_name(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_owned();
    }
    (2..)
        .map(|rank: u32| format!("{name} ({rank})"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| name.to_owned())
}

/// Télécharge les outils cochés que le plan recommande ; un échec n'annule rien.
async fn download_tools(
    context: &ApplyContext<'_>,
    plan: &InstallPlan,
    wanted: &[String],
) -> AppResult<(Vec<String>, Vec<ToolFailure>)> {
    let roots = engine_roots(context.pool, context.config).await?;
    let (mut done, mut failed) = (Vec::new(), Vec::new());
    for tool in plan.tools.iter().filter(|tool| wanted.contains(&tool.id)) {
        match ensure_tool_installed_verified(
            context.pool,
            &roots,
            context.tools_cache,
            &tool.id,
            &tool.version,
            context.sync_policy,
        )
        .await
        {
            Ok(_) => done.push(tool.id.clone()),
            Err(error) => {
                tracing::warn!(tool = %tool.id, ?error, "outil non téléchargé à l'installation");
                failed.push(ToolFailure {
                    id: tool.id.clone(),
                    message: error.to_string(),
                });
            }
        }
    }
    Ok((done, failed))
}

/// Où mène « Utiliser » : la copie de la première cible, ou le plugin lui-même.
fn use_target(target: Option<&ItemRef>, installed: &[InstalledRef]) -> UseTarget {
    let Some(item) = target.and_then(|target| installed.iter().find(|item| item.id == target.id))
    else {
        return UseTarget::None;
    };
    match (item.kind, &item.copy_id) {
        (CatalogItemType::Flow, Some(id)) => UseTarget::Flow { id: id.clone() },
        (CatalogItemType::Script, _) => UseTarget::Script {
            id: item.id.clone(),
        },
        (CatalogItemType::Plugin, _) => UseTarget::Plugin {
            id: item.id.clone(),
        },
        _ => UseTarget::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(id: &str, kind: CatalogItemType, copy_id: Option<&str>) -> InstalledRef {
        InstalledRef {
            id: id.to_owned(),
            kind,
            name: id.to_owned(),
            version: "1.0.0".to_owned(),
            copy_id: copy_id.map(str::to_owned),
        }
    }

    fn target(id: &str) -> ItemRef {
        ItemRef {
            repository_id: "officiel".to_owned(),
            id: id.to_owned(),
        }
    }

    #[test]
    fn a_taken_flow_name_gets_the_next_free_rank() {
        let taken: HashSet<String> = ["Remux".to_owned(), "Remux (2)".to_owned()].into();
        assert_eq!(first_free_name("Remux", &taken), "Remux (3)");
        assert_eq!(first_free_name("Audio", &taken), "Audio");
    }

    #[test]
    fn use_leads_to_the_copy_of_the_first_target() {
        let items = [
            installed("video", CatalogItemType::Plugin, None),
            installed("remux", CatalogItemType::Flow, Some("f-1")),
        ];
        assert_eq!(
            use_target(Some(&target("remux")), &items),
            UseTarget::Flow {
                id: "f-1".to_owned()
            }
        );
        assert_eq!(
            use_target(Some(&target("video")), &items),
            UseTarget::Plugin {
                id: "video".to_owned()
            }
        );
        assert_eq!(use_target(Some(&target("absent")), &items), UseTarget::None);
        assert_eq!(use_target(None, &items), UseTarget::None);
    }

    fn plan(blockers: Vec<forgearr_shared::plugins::store::plan::PlanBlocker>) -> InstallPlan {
        InstallPlan {
            plan_id: "p".to_owned(),
            items: Vec::new(),
            tools: Vec::new(),
            blockers,
            consents: vec![PlanConsent::DiscardSettings {
                id: "video".to_owned(),
            }],
            warnings: Vec::new(),
            suggested_repositories: Vec::new(),
        }
    }

    fn request(plan_id: &str, consents: Vec<PlanConsent>) -> InstallRequest {
        InstallRequest {
            targets: vec![target("video")],
            plan_id: plan_id.to_owned(),
            consents,
            tools: Vec::new(),
        }
    }

    #[test]
    fn only_the_unchanged_unblocked_and_consented_plan_is_applied() {
        let consent = PlanConsent::DiscardSettings {
            id: "video".to_owned(),
        };
        assert!(ensure_accepted(&plan(Vec::new()), &request("p", vec![consent.clone()])).is_ok());
        assert!(matches!(
            ensure_accepted(&plan(Vec::new()), &request("autre", vec![consent.clone()])),
            Err(AppError::PlanOutdated)
        ));
        assert!(matches!(
            ensure_accepted(&plan(Vec::new()), &request("p", Vec::new())),
            Err(AppError::InstallationBlocked(_))
        ));
        let blocked = plan(vec![
            forgearr_shared::plugins::store::plan::PlanBlocker::ModifiedCopy {
                id: "demo".to_owned(),
            },
        ]);
        assert!(matches!(
            ensure_accepted(&blocked, &request("p", vec![consent])),
            Err(AppError::InstallationBlocked(_))
        ));
    }
}
