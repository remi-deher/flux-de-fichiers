//! Plan d'installation, de mise à jour ou de réparation (ADR 0029 §5 à §11) : il lit, n'écrit rien, et se recalcule à
//! l'application.

mod after;
mod claims;
mod completeness;
mod consents;
mod context;
mod declared;
mod findings;
mod impact;
mod listing;
mod needs;
mod states;
#[cfg(test)]
mod tests;
mod tools;
mod walk;

pub use context::PlanContext;
pub use states::InstalledState;

use super::ItemContent;
use crate::{
    config::Config,
    error::{AppError, AppResult},
    opaque_token::sha256_hex,
    plugins::{
        CatalogCache, CatalogItemType, is_plugin_configured, load_manifest,
        repository_copy::COPY_LOCK,
    },
    repository,
};
use after::engine_after;
use findings::Findings;
use forgearr_shared::{
    Permission,
    plugins::store::{
        ItemOrigin, ItemRef,
        plan::{InstallPlan, PlanAction, PlanBlocker, PlanConsent, PlanItem},
    },
};
use serde::Serialize;
use sqlx::SqlitePool;
use walk::{Selected, Walk};

/// Contenu à figer et à inscrire.
#[derive(Clone, Debug)]
pub struct PlannedChange {
    pub action: PlanAction,
    pub origin: ItemOrigin,
    pub content: ItemContent,
}

/// Plan, et ce que son application écrira.
#[derive(Clone, Debug)]
pub struct PreparedPlan {
    pub plan: InstallPlan,
    pub changes: Vec<PlannedChange>,
}

/// Calcule le plan de ces cibles. Le verrou des installations est à l'appelant : partagé pour montrer le plan, seul
/// pour l'appliquer. Refusé tant que la reprise des dépôts n'est pas finie.
pub async fn plan_installation(
    pool: &SqlitePool,
    config: &Config,
    cache: &CatalogCache,
    targets: &[ItemRef],
    may_manage_flows: bool,
) -> AppResult<PreparedPlan> {
    match repository::pending_store_repositories(pool).await? {
        Some(pending) if pending.is_empty() => {}
        pending => {
            return Err(AppError::StoreMigrating(format!(
                "la reprise des éléments déjà utilisés n'est pas finie ({})",
                pending.unwrap_or_default().join(", ")
            )));
        }
    }
    // Une synchronisation ne remplace pas une source pendant sa lecture.
    let _copies = COPY_LOCK.lock().await;
    let context = PlanContext::read(pool, config, cache, may_manage_flows).await?;
    let targets = targets.to_vec();
    tokio::task::spawn_blocking(move || context.plan(&targets))
        .await
        .map_err(|_| AppError::Internal)?
}

impl PlanContext {
    /// Plan de ces cibles. Bloquant.
    pub fn plan(&self, targets: &[ItemRef]) -> AppResult<PreparedPlan> {
        if targets.is_empty() {
            return Err(AppError::Invalid);
        }
        let mut walk = Walk::new(self);
        for target in targets {
            walk.add_target(target)?;
        }
        walk.run()?;
        walk.add_baseline()?;
        let Walk {
            selected,
            mut findings,
            ..
        } = walk;
        self.check(&selected, &mut findings)?;
        let plan = InstallPlan {
            plan_id: plan_id(targets, &selected, &findings)?,
            items: selected.iter().map(|item| self.plan_item(item)).collect(),
            tools: tools::recommended_tools(self, &selected),
            blockers: findings.blockers,
            consents: findings.consents,
            warnings: findings.warnings,
            suggested_repositories: findings.suggested_repositories,
        };
        let changes = selected
            .into_iter()
            .filter_map(|item| {
                let origin = item.origin;
                item.change.map(|change| PlannedChange {
                    action: change.action,
                    origin,
                    content: change.content,
                })
            })
            .collect();
        Ok(PreparedPlan { plan, changes })
    }

    fn check(&self, selected: &[Selected], findings: &mut Findings) -> AppResult<()> {
        let after = engine_after(self, selected);
        completeness::check_files(selected, findings);
        completeness::check_script_copies(self, selected, findings)?;
        completeness::check_requirements(self, selected, &after, findings);
        claims::check_claims(self, selected, findings);
        impact::check_impact(self, selected, &after, findings);
        consents::ask_consents(self, selected, &after, findings);
        let creates_flow = selected
            .iter()
            .any(|item| item.kind == CatalogItemType::Flow && item.change.is_some());
        if creates_flow && !self.may_manage_flows {
            findings.block(PlanBlocker::MissingPermission {
                permission: Permission::ManageFlows,
            });
        }
        Ok(())
    }

    fn plan_item(&self, item: &Selected) -> PlanItem {
        let fields = match &item.change {
            Some(change) => change.declared.settings.clone(),
            None => self
                .engine
                .located(&item.id)
                .and_then(|(root, entry)| load_manifest(root, &entry.manifest_path).ok())
                .and_then(|manifest| manifest.settings_schema)
                .map(|schema| schema.fields)
                .unwrap_or_default(),
        };
        let saved = self
            .settings
            .get(&item.id)
            .filter(|_| !consents::discards_settings(self, item))
            .map(|saved| saved.settings.as_str());
        PlanItem {
            origin: item.origin.clone(),
            id: item.id.clone(),
            kind: item.kind,
            name: item.name.clone(),
            version: item.version.clone(),
            action: item
                .change
                .as_ref()
                .map_or(PlanAction::AlreadyInstalled, |change| change.action),
            reason: item.reason,
            required_by: item.required_by.clone(),
            has_settings: !fields.is_empty(),
            configured: is_plugin_configured(&fields, saved),
        }
    }
}

/// Ce qui fait un plan : un autre contenu, un autre blocage ou un autre accord à donner en fait un autre.
#[derive(Serialize)]
struct Identity<'a> {
    targets: &'a [ItemRef],
    items: Vec<ItemIdentity<'a>>,
    blockers: &'a [PlanBlocker],
    consents: &'a [PlanConsent],
}

#[derive(Serialize)]
struct ItemIdentity<'a> {
    origin: &'a ItemOrigin,
    id: &'a str,
    version: &'a str,
    /// Action et empreinte ; `None` : déjà installé.
    change: Option<(PlanAction, &'a str)>,
}

fn plan_id(targets: &[ItemRef], selected: &[Selected], findings: &Findings) -> AppResult<String> {
    let identity = Identity {
        targets,
        items: selected
            .iter()
            .map(|item| ItemIdentity {
                origin: &item.origin,
                id: &item.id,
                version: &item.version,
                change: item
                    .change
                    .as_ref()
                    .map(|change| (change.action, change.content.content_sha256.as_str())),
            })
            .collect(),
        blockers: &findings.blockers,
        consents: &findings.consents,
    };
    let text = serde_json::to_string(&identity).map_err(|_| AppError::Internal)?;
    Ok(sha256_hex(text))
}
