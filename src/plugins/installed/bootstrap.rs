//! Reprise des dépôts que le moteur lisait avant l'ADR 0029. Sans réseau, depuis les copies déjà sur disque,
//! elle fige ce dont l'installation se sert ; un dépôt reste lu par le moteur tant que ses éléments voulus
//! ne sont pas tous figés au complet. Rejouable : chaque élément est inscrit à part, et ce qui l'est déjà
//! n'est plus touché.

use super::{
    InstallationGuard,
    adopt::{Candidate, adopt, finish_script_copies, prepare},
    lock_installations,
    paths::safe_path,
    survey::Survey,
};
use crate::{
    config::Config,
    error::AppResult,
    plugins::{
        CatalogCache, StoreSource,
        repository_copy::COPY_LOCK,
        sources::{is_awaited, repositories_to_resume, sources_to_resume},
    },
    repository,
};
use forgearr_shared::plugins::store::DEV_SOURCE_ID;
use sqlx::SqlitePool;
use std::collections::HashSet;

/// Dépôt qui vient d'être synchronisé : un fichier encore absent de sa copie l'est vraiment, sauf `withheld`.
#[derive(Clone, Copy, Debug)]
pub struct SyncedRepository<'a> {
    pub id: &'a str,
    /// Fichiers que le serveur a refusés sans dire qu'ils n'existent pas : ils peuvent encore venir.
    pub withheld: &'a [String],
}

impl SyncedRepository<'_> {
    fn withholds_any(&self, missing: &[String]) -> bool {
        self.withheld
            .iter()
            .filter_map(|path| safe_path(path))
            .any(|path| missing.contains(&path))
    }
}

/// Avance la reprise et rend `true` quand elle est finie. Appelée au démarrage, puis après chaque
/// synchronisation, avec le dépôt qui vient de l'être.
pub async fn resume_store(
    pool: &SqlitePool,
    config: &Config,
    cache: &CatalogCache,
    synced: Option<SyncedRepository<'_>>,
) -> AppResult<bool> {
    if crate::instance::worker_setup_only() {
        return Ok(false);
    }
    if is_settled(pool).await? {
        return Ok(true);
    }
    let lock = lock_installations().await;
    let _copies = COPY_LOCK.lock().await;
    let outcome = resume(&lock, pool, config, synced).await;
    // En cas d'échec aussi : des éléments ont pu être inscrits.
    if !matches!(&outcome, Ok(outcome) if !outcome.changed) {
        cache.invalidate().await;
    }
    Ok(outcome?.finished)
}

/// Rien à reprendre, ni copie de script interrompue ?
async fn is_settled(pool: &SqlitePool) -> AppResult<bool> {
    Ok(repository::store_migration_is_done(pool).await?
        && repository::list_installations(pool)
            .await?
            .iter()
            .all(|item| !item.record.copy_pending))
}

struct Outcome {
    finished: bool,
    changed: bool,
}

async fn resume(
    lock: &InstallationGuard,
    pool: &SqlitePool,
    config: &Config,
    synced: Option<SyncedRepository<'_>>,
) -> AppResult<Outcome> {
    let mut changed = finish_script_copies(lock, pool, config).await?;
    let pending = match repository::pending_store_repositories(pool).await? {
        Some(pending) => pending,
        None => {
            let pending = repositories_to_resume(pool, config).await?;
            repository::set_pending_store_repositories(pool, &pending).await?;
            changed = true;
            pending
        }
    };
    let sources = sources_to_resume(pool, config, Some(&pending)).await?;
    let survey = Survey::take(pool, config, &sources).await?;
    let mut awaited = HashSet::new();
    for id in &pending {
        if !survey.reads(id, &sources) && is_awaited(pool, config, id).await? {
            awaited.insert(id.as_str());
        }
    }
    let scope = Scope {
        pending: &pending,
        sources: &sources,
        awaited,
        synced,
    };
    let Selection {
        candidates,
        mut held,
        incomplete,
        waited_for,
    } = select(pool, config, &survey, &scope).await;
    for (source, candidate) in candidates {
        let source_id = source.origin.repository_id.as_str();
        if incomplete.contains(source_id) && !candidate.is_recorded() {
            continue;
        }
        let item_id = candidate.id().to_owned();
        match adopt(lock, pool, config, source, candidate).await {
            Ok(()) => changed = true,
            // Les autres continuent ; celui-ci garde son dépôt dans le moteur.
            Err(error) => {
                tracing::warn!(item = %item_id, repository = source_id, ?error, "élément utilisé pas encore repris");
                held.insert(source_id);
            }
        }
    }

    let mut remaining = pending.clone();
    remaining.retain(|id| {
        let id = id.as_str();
        if survey.reads(id, &sources) {
            held.contains(id)
        } else {
            // Sans copie lisible, il n'est attendu que s'il peut porter ce qui manque encore.
            scope.awaited.contains(id) && (survey.unresolved || waited_for.contains(id))
        }
    });
    if remaining != pending {
        repository::set_pending_store_repositories(pool, &remaining).await?;
        changed = true;
    }
    if remaining.is_empty() {
        tracing::info!("reprise des éléments installés terminée");
    } else {
        tracing::info!(repositories = ?remaining, "reprise des éléments installés en cours");
    }
    Ok(Outcome {
        finished: remaining.is_empty(),
        changed,
    })
}

/// Ce que la reprise peut lire, et ce qu'elle attend encore.
struct Scope<'a> {
    /// Dépôts à reprendre, par priorité.
    pending: &'a [String],
    sources: &'a [StoreSource],
    /// Dépôts à reprendre sans copie lisible, qui peuvent encore en recevoir une.
    awaited: HashSet<&'a str>,
    synced: Option<SyncedRepository<'a>>,
}

impl<'a> Scope<'a> {
    /// Dépôts attendus plus prioritaires que cette source.
    fn awaited_before(&self, source_id: &str) -> Vec<&'a str> {
        self.pending
            .iter()
            .map(String::as_str)
            .take_while(|id| *id != source_id)
            .filter(|id| self.awaited.contains(id))
            .collect()
    }
}

/// Éléments à figer, et dépôts à garder dans le moteur.
#[derive(Default)]
struct Selection<'a> {
    candidates: Vec<(&'a StoreSource, Candidate)>,
    /// Sources dont un élément voulu n'est pas encore figé.
    held: HashSet<&'a str>,
    /// Sources qu'une synchronisation peut encore compléter. Rien n'y est figé avant : leurs éléments restent
    /// de la même publication.
    incomplete: HashSet<&'a str>,
    /// Dépôts sans copie dont un plugin configuré attend la lecture.
    waited_for: HashSet<&'a str>,
}

async fn select<'a>(
    pool: &SqlitePool,
    config: &Config,
    survey: &Survey,
    scope: &Scope<'a>,
) -> Selection<'a> {
    let mut selection = Selection::default();
    for item_id in &survey.wanted {
        let Some(source) = survey.source_of(item_id, scope.sources) else {
            continue;
        };
        let source_id = source.origin.repository_id.as_str();
        let candidate = match prepare(pool, config, source, item_id).await {
            Ok(None) => continue,
            Ok(Some(candidate)) => candidate,
            Err(error) => {
                tracing::warn!(item = %item_id, repository = source_id, ?error, "élément utilisé pas encore repris");
                selection.held.insert(source_id);
                continue;
            }
        };
        // Ses réglages, secrets compris, vont au dépôt prioritaire, qui peut porter ce plugin.
        if survey.configured.contains(item_id) && !candidate.is_recorded() {
            let awaited = scope.awaited_before(source_id);
            if !awaited.is_empty() {
                tracing::warn!(item = %item_id, repository = source_id, ?awaited, "plugin configuré : reprise après la lecture d'un dépôt prioritaire");
                selection.held.insert(source_id);
                selection.waited_for.extend(awaited);
                continue;
            }
        }
        let missing = candidate.missing();
        if !missing.is_empty() {
            let synced = scope.synced.filter(|synced| synced.id == source_id);
            let is_fresh = synced.is_some() || source_id == DEV_SOURCE_ID;
            if !is_fresh {
                selection.incomplete.insert(source_id);
            }
            if !is_fresh || synced.is_some_and(|synced| synced.withholds_any(missing)) {
                tracing::warn!(item = %item_id, repository = source_id, ?missing, "fichiers pas encore copiés");
                selection.held.insert(source_id);
                continue;
            }
            tracing::warn!(item = %item_id, repository = source_id, ?missing, "élément repris sans des fichiers que son dépôt ne publie pas");
        }
        selection.candidates.push((source, candidate));
    }
    selection
}
