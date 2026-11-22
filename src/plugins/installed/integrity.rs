//! Copies figées lisibles par le moteur. Une copie est vérifiée en entier une fois par démarrage, puis à
//! chaque lecture tant qu'elle est absente ou altérée : une réparation se voit aussitôt.

use super::{frozen_is_intact, installed_root};
use crate::{
    plugins::sources::has_index,
    repository::{Installation, InstallationRecord},
};
use forgearr_shared::plugins::vocab::CatalogItemType;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, PoisonError},
};

/// Élément inscrit dont le moteur ne peut pas lire la copie : « à réparer ».
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamagedItem {
    pub id: String,
    /// Il portait une sonde ou une opération par défaut.
    pub serves_every_task: bool,
}

impl DamagedItem {
    fn of(record: &InstallationRecord) -> Self {
        Self {
            id: record.item_id.clone(),
            serves_every_task: record.serves_every_task,
        }
    }
}

/// Dernier état connu de chaque copie figée : lisible ou non.
static READABLE: LazyLock<Mutex<HashMap<PathBuf, bool>>> = LazyLock::new(Mutex::default);

/// Le moteur peut-il lire cette copie figée ? Bloquant.
pub(super) fn is_readable(root: &Path, record: &InstallationRecord) -> bool {
    let known = READABLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(root)
        .copied();
    // Présente et déjà vérifiée : son contenu n'est pas relu.
    let readable = has_index(root)
        && (known == Some(true) || frozen_is_intact(root, &record.item_id, &record.content_sha256));
    if known != Some(readable) {
        if !readable {
            tracing::error!(item = %record.item_id, root = %root.display(), "copie figée absente ou altérée : élément à réparer");
        }
        READABLE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(root.to_path_buf(), readable);
    }
    readable
}

/// Copies figées que le moteur lit, dans l'ordre des inscriptions, et éléments dont la copie manque, est
/// altérée ou (script) reste à écrire. Bloquant.
pub(in crate::plugins) fn frozen_roots(
    installed_dir: &Path,
    installations: &[Installation],
) -> (Vec<PathBuf>, Vec<DamagedItem>) {
    let mut roots = Vec::new();
    let mut damaged = Vec::new();
    for Installation { record, .. } in installations {
        if record.kind == CatalogItemType::Script && record.copy_pending {
            damaged.push(DamagedItem::of(record));
        }
        if !record.kind.runs_from_frozen_copy() {
            continue;
        }
        match installed_root(installed_dir, record).filter(|root| is_readable(root, record)) {
            Some(root) => roots.push(root),
            None => damaged.push(DamagedItem::of(record)),
        }
    }
    (roots, damaged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::installed::{
        freeze_item, lock_installations, read_item, test_support::Repository,
    };
    use forgearr_shared::plugins::store::ItemOrigin;

    fn installation(id: &str, kind: CatalogItemType, sha: &str) -> Installation {
        Installation {
            record: InstallationRecord {
                item_id: id.to_owned(),
                kind,
                version: "1.0.0".to_owned(),
                content_sha256: sha.to_owned(),
                origin: ItemOrigin {
                    repository_id: "repo.test".to_owned(),
                    repository_url: "https://example.org/index.json".to_owned(),
                    repository_name: "Test".to_owned(),
                },
                copy_id: None,
                copy_pending: kind == CatalogItemType::Script,
                serves_every_task: false,
            },
            installed_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[tokio::test]
    async fn a_missing_or_altered_copy_is_not_read_until_it_is_repaired() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let dir = tempfile::tempdir()?;
        let content = read_item(repository.path(), "demo")?;
        let sha = content.content_sha256.clone();
        let items = [installation("demo", CatalogItemType::Plugin, &sha)];
        let damaged = || (Vec::new(), vec![DamagedItem::of(&items[0].record)]);
        assert_eq!(frozen_roots(dir.path(), &items), damaged());

        let lock = lock_installations().await;
        let origin = &items[0].record.origin;
        let root = freeze_item(&lock, dir.path(), content.clone(), origin).await?;
        let readable = (vec![root.clone()], Vec::new());
        assert_eq!(frozen_roots(dir.path(), &items), readable);

        std::fs::remove_dir_all(&root)?;
        assert_eq!(frozen_roots(dir.path(), &items), damaged());
        freeze_item(&lock, dir.path(), content, origin).await?;
        assert_eq!(frozen_roots(dir.path(), &items), readable);
        Ok(())
    }

    #[test]
    fn flows_and_scripts_do_not_run_from_their_frozen_copy() {
        let sha = "a".repeat(64);
        let flow = [installation("flux", CatalogItemType::Flow, &sha)];
        let absent = Path::new("/absent");
        assert_eq!(frozen_roots(absent, &flow), (Vec::new(), Vec::new()));
        // Script dont la copie modifiable n'est pas encore écrite.
        let script = [installation("script", CatalogItemType::Script, &sha)];
        assert_eq!(
            frozen_roots(absent, &script),
            (Vec::new(), vec![DamagedItem::of(&script[0].record)])
        );
    }
}
