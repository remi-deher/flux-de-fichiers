//! Copie modifiable d'un script installé, dans la collection locale : mêmes identifiants, bloc et opération,
//! donc mêmes flux (ADR 0029).

use super::{
    ItemContent,
    freeze::{create_durable_dir, staging_path, sync_dir, write_files},
    paths::{folder_of, safe_path},
    read_item,
};
use crate::{
    error::{AppError, AppResult},
    plugins::local::{INSTALLED_SCRIPTS_FOLDER, rebuild_local_index},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Nom que `rebuild_local_index` cherche dans chaque dossier.
const MANIFEST_FILE: &str = "manifest.json";

/// Dossier de la copie modifiable d'un script installé.
pub(super) fn script_copy_dir(local_dir: &Path, item_id: &str) -> PathBuf {
    local_dir.join(INSTALLED_SCRIPTS_FOLDER).join(item_id)
}

/// Écrit la copie modifiable d'un script depuis sa copie figée. Sans effet si elle est déjà là, même modifiée ;
/// `Conflict` si autre chose occupe son identifiant ou son dossier. Bloquant, sous le verrou des
/// installations.
pub(super) fn write_script_copy(frozen: &Path, local_dir: &Path, item_id: &str) -> AppResult<()> {
    if is_already_copied(local_dir, item_id)? {
        return Ok(());
    }
    let content = read_item(frozen, item_id)?;
    let files = files_of_copy(&content)?;
    let destination = script_copy_dir(local_dir, item_id);
    create_durable_dir(local_dir)?;
    let folder = local_dir.join(INSTALLED_SCRIPTS_FOLDER);
    create_durable_dir(&folder)?;
    // Nom fixe : le reste d'un arrêt brutal est remplacé à l'essai suivant.
    let staging = local_dir.join(format!(".tmp-{item_id}"));
    remove_dir_if_present(&staging)?;
    write_files(&staging, files.into_iter())?;
    fs::rename(&staging, &destination)?;
    sync_dir(&folder)?;
    rebuild_local_index(local_dir)?;
    if !is_same(local_dir, item_id, &content) {
        fs::remove_dir_all(&destination)?;
        rebuild_local_index(local_dir)?;
        return Err(AppError::InvalidManifest(format!(
            "{item_id} : copie du script différente de l'original"
        )));
    }
    Ok(())
}

/// Refuse un script qui n'est pas copiable, en écrivant sa copie à part : rien ne doit être inscrit pour lui.
/// Bloquant, sous le verrou des installations.
pub(super) fn check_script_copy(
    installed_dir: &Path,
    frozen: &Path,
    local_dir: &Path,
    item_id: &str,
) -> AppResult<()> {
    if is_already_copied(local_dir, item_id)? {
        return Ok(());
    }
    let trial = staging_path(installed_dir);
    let written = write_script_copy(frozen, &trial, item_id);
    let _ = fs::remove_dir_all(&trial);
    written
}

/// La copie modifiable est-elle là ? `Conflict` si autre chose occupe son dossier ou son identifiant.
fn is_already_copied(local_dir: &Path, item_id: &str) -> AppResult<bool> {
    if script_copy_dir(local_dir, item_id).try_exists()? {
        // Un arrêt a pu précéder la mise à jour de l'index.
        rebuild_local_index(local_dir)?;
        // Le dossier n'apparaît que complet : lisible sous ce nom, c'est la copie, modifiée depuis ou non.
        let folder = format!("{INSTALLED_SCRIPTS_FOLDER}/{item_id}");
        let is_copy = read_item(local_dir, item_id).is_ok_and(|copy| {
            safe_path(&copy.entry.manifest_path).is_some_and(|path| folder_of(&path) == folder)
        });
        return if is_copy {
            Ok(true)
        } else {
            Err(AppError::Conflict)
        };
    }
    match read_item(local_dir, item_id) {
        Err(AppError::NotFound) => Ok(false),
        // L'identifiant est déjà pris, quel que soit l'état de ce qui le porte.
        _ => Err(AppError::Conflict),
    }
}

/// Fichiers relatifs au dossier du script ; refus si l'un d'eux est ailleurs.
fn files_of_copy(content: &ItemContent) -> AppResult<Vec<(&str, &[u8])>> {
    let item_id = &content.entry.id;
    let refused = |reason: &str| AppError::InvalidManifest(format!("{item_id} : {reason}"));
    let manifest = safe_path(&content.entry.manifest_path)
        .ok_or_else(|| refused("chemin de manifeste refusé"))?;
    let folder = folder_of(&manifest);
    if folder.is_empty() || manifest != format!("{folder}/{MANIFEST_FILE}") {
        return Err(refused("un script doit avoir son dossier et son manifeste"));
    }
    content
        .files()
        .iter()
        .map(|(path, bytes)| {
            path.strip_prefix(folder)
                .and_then(|rest| rest.strip_prefix('/'))
                .map(|relative| (relative, bytes.as_slice()))
                .ok_or_else(|| refused("fichier hors du dossier du script"))
        })
        .collect()
}

fn is_same(local_dir: &Path, item_id: &str, content: &ItemContent) -> bool {
    read_item(local_dir, item_id).is_ok_and(|copy| copy.has_same_files_as(content))
}

fn remove_dir_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::installed::test_support::{OPERATION, Repository, write};
    use serde_json::json;

    /// Le plugin de test, présenté comme un script.
    fn script() -> anyhow::Result<Repository> {
        let repository = Repository::with_plugin_in("scripts/demo")?;
        repository.edit_entry("demo", "type", json!("script"))?;
        let manifest = repository.path().join("scripts/demo/manifest.json");
        let text = std::fs::read_to_string(&manifest)?;
        std::fs::write(&manifest, text.replace(r#""plugin""#, r#""script""#))?;
        Ok(repository)
    }

    #[test]
    fn a_script_keeps_its_identifiers_and_runs_from_the_local_library() -> anyhow::Result<()> {
        let repository = script()?;
        let local = tempfile::tempdir()?;
        // Un script de l'utilisateur du même nom garde son dossier.
        write(local.path(), "scripts/demo/manifest.json", "{}")?;
        for _ in 0..2 {
            write_script_copy(repository.path(), local.path(), "demo")?;
        }
        let roots = [local.path().to_path_buf()];
        let registry = crate::flows::OperationRegistry::load_from_roots(&roots);
        assert_eq!(registry.owner_of(OPERATION), Some("demo"));
        assert_eq!(registry.operation_id_for_kind("demo-run"), Some(OPERATION));
        assert!(local.path().join("scripts/demo/manifest.json").is_file());
        Ok(())
    }

    #[test]
    fn an_interrupted_copy_is_finished_and_a_modified_one_is_kept() -> anyhow::Result<()> {
        let repository = script()?;
        let local = tempfile::tempdir()?;
        write(local.path(), ".tmp-demo/reste", "x")?;
        write_script_copy(repository.path(), local.path(), "demo")?;
        assert!(!local.path().join(".tmp-demo").exists());

        // Arrêt entre le renommage et l'index.
        std::fs::remove_file(local.path().join("index.json"))?;
        write_script_copy(repository.path(), local.path(), "demo")?;
        assert!(read_item(local.path(), "demo").is_ok());

        let code = script_copy_dir(local.path(), "demo").join("run.py");
        std::fs::write(&code, "print('à moi')")?;
        write_script_copy(repository.path(), local.path(), "demo")?;
        assert_eq!(std::fs::read_to_string(&code)?, "print('à moi')");
        // Sans la copie figée aussi : rien n'est à écrire.
        write_script_copy(Path::new("/absent"), local.path(), "demo")?;

        // Autre chose dans ce dossier n'est ni accepté ni écrasé.
        let manifest = script_copy_dir(local.path(), "demo").join(MANIFEST_FILE);
        std::fs::write(&manifest, "{")?;
        let refused = write_script_copy(repository.path(), local.path(), "demo");
        assert!(matches!(refused, Err(AppError::Conflict)));
        assert_eq!(std::fs::read_to_string(&manifest)?, "{");
        Ok(())
    }

    #[test]
    fn a_script_spread_over_several_folders_is_not_copied() -> anyhow::Result<()> {
        let repository = script()?;
        write(repository.path(), "blocs/run.json", "{}")?;
        repository.edit_entry("demo.run", "manifestPath", json!("blocs/run.json"))?;
        let local = tempfile::tempdir()?;
        let installed = tempfile::tempdir()?;
        let refused = check_script_copy(installed.path(), repository.path(), local.path(), "demo");
        assert!(matches!(refused, Err(AppError::InvalidManifest(_))));
        assert!(!script_copy_dir(local.path(), "demo").exists());
        Ok(())
    }

    #[test]
    fn a_refused_script_leaves_nothing_behind() -> anyhow::Result<()> {
        // Le manifeste ne porte pas l'identifiant de l'index : la copie serait introuvable.
        let repository = script()?;
        let manifest = repository.path().join("scripts/demo/manifest.json");
        let text = std::fs::read_to_string(&manifest)?;
        std::fs::write(&manifest, text.replace(r#""id":"demo""#, r#""id":"autre""#))?;
        let (installed, local) = (tempfile::tempdir()?, tempfile::tempdir()?);
        let check = |repository: &Repository| {
            check_script_copy(installed.path(), repository.path(), local.path(), "demo")
        };
        assert!(matches!(
            check(&repository),
            Err(AppError::InvalidManifest(_))
        ));
        check(&script()?)?;
        for dir in [installed.path(), local.path()] {
            assert_eq!(std::fs::read_dir(dir)?.count(), 0);
        }
        Ok(())
    }
}
