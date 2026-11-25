//! Copies figées des éléments installés (ADR 0029) : `/config/installed/<id>.<empreinte>/`, un
//! mini-dépôt immuable par élément. La base fait autorité ; un dossier non inscrit est un reste.

mod adopt;
mod availability;
mod bootstrap;
mod files;
mod freeze;
mod integrity;
mod paths;
mod plan;
mod remove;
mod script_copy;
mod survey;
mod sweep;
#[cfg(test)]
mod test_support;
mod usage;

pub use availability::{held_flows, unreadable_item};
pub use bootstrap::{SyncedRepository, resume_store};
pub use files::{ItemContent, read_item};
pub(super) use freeze::replace_file;
pub use freeze::{freeze_item, frozen_is_intact};
pub use integrity::DamagedItem;
pub(super) use integrity::frozen_roots;
pub use plan::{InstalledState, PlanContext, PlannedChange, PreparedPlan, plan_installation};
pub use remove::{delete_collection_item, ensure_unused};
pub use script_copy::{ScriptCopy, compare_script_copy};
pub(super) use survey::configured_plugins;
pub use sweep::sweep_orphans;
pub use usage::{EngineCatalog, UsageSources, UsedElements, elements_used_by, item_usage};

use super::is_valid_repository_id;
use crate::{opaque_token::is_valid_sha256_hex, repository::InstallationRecord};
use std::path::{Path, PathBuf};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Plan, gel, inscription, retrait et balayage s'excluent. Ce qui ne fait que se servir des éléments
/// (enregistrer un flux, par exemple) le partage. Jamais tenu pendant un téléchargement. Non réentrant.
static INSTALLATION_LOCK: RwLock<()> = RwLock::const_new(());

/// Preuve que le verrou des installations est tenu.
pub struct InstallationGuard {
    _lock: RwLockWriteGuard<'static, ()>,
}

/// Prend seul le verrou des installations, jusqu'à la validation en base.
pub async fn lock_installations() -> InstallationGuard {
    InstallationGuard {
        _lock: INSTALLATION_LOCK.write().await,
    }
}

/// Empêche toute installation et tout retrait, sans exclure les autres lecteurs.
pub async fn share_installations() -> RwLockReadGuard<'static, ()> {
    INSTALLATION_LOCK.read().await
}

/// Nom du dossier figé : `<id>.<empreinte complète>` ; `None` si l'un des deux n'est pas sûr.
fn storage_dir_name(item_id: &str, content_sha256: &str) -> Option<String> {
    (is_valid_repository_id(item_id) && is_valid_sha256_hex(content_sha256))
        .then(|| format!("{item_id}.{content_sha256}"))
}

fn is_storage_dir_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(item_id, sha)| storage_dir_name(item_id, sha).is_some())
}

/// Dossier figé d'un élément inscrit, qu'il existe ou non.
pub fn installed_root(installed_dir: &Path, record: &InstallationRecord) -> Option<PathBuf> {
    storage_dir_name(&record.item_id, &record.content_sha256).map(|name| installed_dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opaque_token::sha256_hex;

    #[test]
    fn a_storage_name_is_safe_and_recognised() {
        let sha = sha256_hex("contenu");
        for id in ["video", "flow.template.x", "a_b-c"] {
            let name = storage_dir_name(id, &sha).unwrap_or_default();
            assert!(is_storage_dir_name(&name), "{id}");
        }
        assert_eq!(storage_dir_name("../evil", &sha), None);
        assert_eq!(storage_dir_name(".cache", &sha), None);
        assert_eq!(storage_dir_name("video", "../x"), None);
        for other in ["video", "video.abc", ".tmp-x", "notes de l'utilisateur"] {
            assert!(!is_storage_dir_name(other), "{other}");
        }
    }
}
