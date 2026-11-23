//! Écriture d'une copie figée : à part, synchronisée sur disque, puis renommée. La base l'inscrit
//! ensuite ; un arrêt entre les deux ne laisse qu'un dossier à balayer.

use super::{
    InstallationGuard, ItemContent, read_item, storage_dir_name,
    sweep::{STAGING_PREFIX, orphan_marker},
};
use crate::{
    error::{AppError, AppResult},
    opaque_token::sha256_hex,
    plugins::sources::INDEX_RELATIVE_PATH,
};
use forgearr_shared::plugins::store::ItemOrigin;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

/// Champ d'index d'un mini-dépôt : l'origine de l'élément.
const FROZEN_FROM: &str = "frozenFrom";

/// Fige `content` dans `installed_dir` et rend son dossier. Sans effet si une copie intacte y est déjà ;
/// une copie altérée est remplacée. Le verrou reste tenu jusqu'à l'inscription en base, sans quoi le
/// balayage prendrait la copie pour un reste.
///
/// `content.missing` n'est pas contrôlé : seule la reprise fige un élément incomplet.
pub async fn freeze_item(
    _lock: &InstallationGuard,
    installed_dir: &Path,
    content: ItemContent,
    origin: &ItemOrigin,
) -> AppResult<PathBuf> {
    let installed_dir = installed_dir.to_path_buf();
    let origin = origin.clone();
    tokio::task::spawn_blocking(move || freeze(&installed_dir, &content, &origin))
        .await
        .map_err(|_| AppError::Internal)?
}

/// Le dossier figé existe-t-il encore, avec exactement ce contenu ? Lit tous ses fichiers.
pub fn frozen_is_intact(root: &Path, item_id: &str, content_sha256: &str) -> bool {
    read_item(root, item_id).is_ok_and(|content| content.content_sha256 == content_sha256)
}

fn freeze(installed_dir: &Path, content: &ItemContent, origin: &ItemOrigin) -> AppResult<PathBuf> {
    let item_id = &content.entry.id;
    let sha = &content.content_sha256;
    let name = storage_dir_name(item_id, sha).ok_or(AppError::Invalid)?;
    let destination = installed_dir.join(&name);
    create_durable_dir(installed_dir)?;
    // Redevenue utile : son délai de garde repartira de zéro.
    match fs::remove_file(orphan_marker(installed_dir, &name)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    if frozen_is_intact(&destination, item_id, sha) {
        // Même contenu, figé depuis un autre dépôt : seule l'origine que dit son index change.
        if !is_frozen_from(&destination, origin) {
            let index = frozen_index(content, origin).to_string();
            replace_file(&destination.join(INDEX_RELATIVE_PATH), index.as_bytes())?;
        }
        sync_dir(installed_dir)?;
        return Ok(destination);
    }
    let staging = staging_path(installed_dir);
    let published = write_tree(&staging, content, origin)
        .and_then(|()| substitute(installed_dir, &staging, &destination));
    if let Err(error) = published {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.into());
    }
    // Relue depuis le disque : une copie qui ne redonne pas son empreinte ne doit pas être inscrite.
    if !frozen_is_intact(&destination, item_id, sha) {
        return Err(AppError::InvalidManifest(format!(
            "{item_id} : copie figée différente de l'original"
        )));
    }
    Ok(destination)
}

fn is_frozen_from(root: &Path, origin: &ItemOrigin) -> bool {
    fs::read_to_string(root.join(INDEX_RELATIVE_PATH))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|index| ItemOrigin::deserialize(index.get(FROZEN_FROM)?).ok())
        .is_some_and(|known| known.is_same_as(origin))
}

pub(super) fn staging_path(installed_dir: &Path) -> PathBuf {
    installed_dir.join(format!("{STAGING_PREFIX}{}", uuid::Uuid::new_v4().simple()))
}

/// Met la copie en place ; une copie altérée du même nom est écartée, puis retirée.
fn substitute(installed_dir: &Path, staging: &Path, destination: &Path) -> io::Result<()> {
    let damaged = staging_path(installed_dir);
    let had_damaged = match fs::rename(destination, &damaged) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };
    if let Err(error) = fs::rename(staging, destination) {
        if had_damaged {
            let _ = fs::rename(&damaged, destination);
        }
        return Err(error);
    }
    sync_dir(installed_dir)?;
    if had_damaged {
        let _ = fs::remove_dir_all(&damaged);
    }
    Ok(())
}

/// Écrit les fichiers et l'index, et les synchronise avec leurs dossiers.
fn write_tree(staging: &Path, content: &ItemContent, origin: &ItemOrigin) -> io::Result<()> {
    let index = frozen_index(content, origin).to_string();
    let files = content
        .files()
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .chain([(INDEX_RELATIVE_PATH, index.as_bytes())]);
    write_files(staging, files)
}

/// Écrit des fichiers dans un nouveau dossier, synchronisés avec leurs dossiers.
pub(super) fn write_files<'a>(
    root: &Path,
    files: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> io::Result<()> {
    fs::create_dir(root)?;
    let mut folders = BTreeSet::from([root.to_path_buf()]);
    for (path, bytes) in files {
        let target = root.join(path);
        for folder in target
            .ancestors()
            .skip(1)
            .take_while(|folder| *folder != root)
        {
            folders.insert(folder.to_path_buf());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_durable(&target, bytes)?;
    }
    // Les plus profonds d'abord.
    folders.iter().rev().try_for_each(|folder| sync_dir(folder))
}

/// Index du mini-dépôt. L'origine permet de retrouver une inscription perdue.
fn frozen_index(content: &ItemContent, origin: &ItemOrigin) -> Value {
    let files_sha256: serde_json::Map<String, Value> = content
        .files()
        .iter()
        .map(|(path, bytes)| (path.clone(), Value::String(sha256_hex(bytes))))
        .collect();
    json!({
        "schemaVersion": content.schema_version,
        "generatedFor": "installed",
        FROZEN_FROM: origin,
        "filesSha256": files_sha256,
        "entries": content.raw_entries,
    })
}

fn write_durable(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Remplace un fichier d'un seul renommage, synchronisé sur disque.
pub(in crate::plugins) fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let unique = uuid::Uuid::new_v4().simple();
    let staging = path.with_extension(format!("{unique}.tmp"));
    let replaced = write_durable(&staging, bytes).and_then(|()| fs::rename(&staging, path));
    if replaced.is_err() {
        let _ = fs::remove_file(&staging);
    }
    replaced?;
    path.parent().map_or(Ok(()), sync_dir)
}

pub(super) fn sync_dir(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

pub(super) fn create_durable_dir(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(path)?;
    path.parent().map_or(Ok(()), sync_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        flows::{OperationRegistry, OperationRuntime},
        plugins::{
            flow_block_catalog,
            installed::{
                lock_installations,
                test_support::{CODE, OPERATION, Repository},
            },
            list_catalog,
        },
    };

    fn origin() -> ItemOrigin {
        ItemOrigin {
            repository_id: "repo.test".to_owned(),
            repository_url: "https://example.org/index.json".to_owned(),
            repository_name: "Test".to_owned(),
        }
    }

    fn names(dir: &Path) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        Ok(names)
    }

    #[tokio::test]
    async fn a_frozen_copy_is_a_catalogue_of_its_own() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let config = tempfile::tempdir()?;
        let installed = config.path().join("installed");
        let content = read_item(repository.path(), "demo")?;
        let sha = content.content_sha256.clone();

        let lock = lock_installations().await;
        let frozen = freeze_item(&lock, &installed, content.clone(), &origin()).await?;
        assert_eq!(names(&installed)?, [format!("demo.{sha}")]);
        assert!(frozen_is_intact(&frozen, "demo", &sha));

        // Lue comme n'importe quel dossier de catalogue : l'élément et ses blocs, rien d'autre.
        let roots = [frozen.clone()];
        let ids: Vec<String> = list_catalog(&roots)
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, ["demo", "demo.run"]);
        let blocks = flow_block_catalog(&roots);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].owner.as_deref(), Some("demo"));

        // Le dépôt d'origine peut changer ou disparaître ; figer de nouveau ne réécrit rien.
        drop(repository);
        assert!(frozen_is_intact(&frozen, "demo", &sha));
        let again = freeze_item(&lock, &installed, content.clone(), &origin()).await?;
        assert_eq!(again, frozen);
        assert_eq!(names(&installed)?.len(), 1);

        // Le même contenu venu d'ailleurs ne garde pas l'ancienne origine.
        let moved = ItemOrigin {
            repository_url: "https://autre.example/index.json".to_owned(),
            ..origin()
        };
        let before = names(&frozen)?;
        freeze_item(&lock, &installed, content, &moved).await?;
        assert!(is_frozen_from(&frozen, &moved) && !is_frozen_from(&frozen, &origin()));
        assert!(frozen_is_intact(&frozen, "demo", &sha));
        assert_eq!(names(&frozen)?, before);
        Ok(())
    }

    #[tokio::test]
    async fn a_damaged_copy_is_detected_and_replaced() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let config = tempfile::tempdir()?;
        let installed = config.path().join("installed");
        let content = read_item(repository.path(), "demo")?;
        let sha = content.content_sha256.clone();
        let lock = lock_installations().await;
        let frozen = freeze_item(&lock, &installed, content.clone(), &origin()).await?;
        let damages: [fn(&Path) -> io::Result<()>; 3] = [
            |root| fs::write(root.join("plugins/demo/run.py"), "print(0)"),
            |root| fs::remove_file(root.join("plugins/demo/blocks/run.json")),
            |root| fs::remove_dir_all(root.join("plugins")),
        ];
        for damage in damages {
            damage(&frozen)?;
            assert!(!frozen_is_intact(&frozen, "demo", &sha));
            freeze_item(&lock, &installed, content.clone(), &origin()).await?;
            assert!(frozen_is_intact(&frozen, "demo", &sha));
            assert_eq!(names(&installed)?.len(), 1);
        }
        Ok(())
    }

    #[tokio::test]
    async fn an_incomplete_item_can_be_frozen_and_stays_marked() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        fs::remove_file(repository.path().join("plugins/demo/run.py"))?;
        let config = tempfile::tempdir()?;
        let content = read_item(repository.path(), "demo")?;
        let missing = content.missing.clone();
        let sha = content.content_sha256.clone();
        let lock = lock_installations().await;
        let frozen = freeze_item(&lock, config.path(), content, &origin()).await?;
        let again = read_item(&frozen, "demo")?;
        assert_eq!(again.missing, missing);
        assert_eq!(again.content_sha256, sha);
        Ok(())
    }

    /// Le code d'un script vient de sa copie figée, sans le dépôt.
    #[tokio::test]
    async fn the_engine_reads_operations_from_the_frozen_copy() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let config = tempfile::tempdir()?;
        let lock = lock_installations().await;
        let content = read_item(repository.path(), "demo")?;
        let frozen = freeze_item(&lock, config.path(), content, &origin()).await?;
        drop(repository);
        let registry = OperationRegistry::load_from_roots(&[frozen]);
        let code = registry
            .get(OPERATION)
            .and_then(|operation| match &operation.runtime {
                OperationRuntime::Script(runtime) => runtime.code.as_deref(),
                _ => None,
            });
        assert_eq!(code, Some(CODE));
        Ok(())
    }
}
