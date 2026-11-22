//! Copie sur disque d'un dépôt de plugins : publication par renommages, reprise après un arrêt
//! brutal entre deux renommages, retrait.

use super::SyncError;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Publication, ajout et retrait d'un dépôt s'excluent : une copie n'est jamais publiée pour un
/// dépôt retiré entre-temps.
pub(super) static COPY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Dossier en cours d'écriture (`.<dépôt>.tmp-<uuid>`).
const STAGING_MARK: &str = ".tmp-";
/// Copie remplacée, gardée le temps du renommage (`.<dépôt>.old-<uuid>`).
const PREVIOUS_MARK: &str = ".old-";

/// Écrit les fichiers dans un dossier temporaire, puis le substitue d'un seul renommage.
pub(super) async fn publish(
    repos_dir: &Path,
    repository_id: &str,
    files: &[(String, Vec<u8>)],
) -> Result<(), SyncError> {
    tokio::fs::create_dir_all(repos_dir).await?;
    let unique = uuid::Uuid::new_v4().simple();
    let staging = repos_dir.join(format!(".{repository_id}{STAGING_MARK}{unique}"));
    for (relative, content) in files {
        let target = staging.join(relative);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, content).await?;
    }
    let destination = repos_dir.join(repository_id);
    let previous = repos_dir.join(format!(".{repository_id}{PREVIOUS_MARK}{unique}"));
    let had_previous = tokio::fs::try_exists(&destination).await?;
    if had_previous {
        tokio::fs::rename(&destination, &previous).await?;
    }
    if let Err(error) = tokio::fs::rename(&staging, &destination).await {
        if had_previous {
            let _ = tokio::fs::rename(&previous, &destination).await;
        }
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(SyncError::Storage(error));
    }
    if had_previous {
        let _ = tokio::fs::remove_dir_all(&previous).await;
    }
    Ok(())
}

/// Remet en place la copie remplacée si un arrêt a précédé la nouvelle, puis retire les restes.
/// Rend `true` si une copie a été remise en place.
pub(super) async fn recover(repos_dir: &Path, repository_id: &str) -> std::io::Result<bool> {
    let restored = restore_previous(repos_dir, repository_id).await?;
    remove_leftovers(repos_dir, repository_id).await?;
    Ok(restored)
}

/// Efface la copie d'un dépôt et ses restes ; sans effet si elle n'existe pas.
pub(super) async fn remove(repos_dir: &Path, repository_id: &str) -> std::io::Result<()> {
    if let Err(error) = tokio::fs::remove_dir_all(repos_dir.join(repository_id)).await
        && error.kind() != ErrorKind::NotFound
    {
        return Err(error);
    }
    remove_leftovers(repos_dir, repository_id).await
}

async fn restore_previous(repos_dir: &Path, repository_id: &str) -> std::io::Result<bool> {
    let destination = repos_dir.join(repository_id);
    if tokio::fs::try_exists(&destination).await? {
        return Ok(false);
    }
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for path in leftovers(repos_dir, repository_id, PREVIOUS_MARK).await? {
        let modified = tokio::fs::metadata(&path).await?.modified()?;
        if newest.as_ref().is_none_or(|(known, _)| modified > *known) {
            newest = Some((modified, path));
        }
    }
    let Some((_, previous)) = newest else {
        return Ok(false);
    };
    tokio::fs::rename(previous, destination).await?;
    Ok(true)
}

async fn remove_leftovers(repos_dir: &Path, repository_id: &str) -> std::io::Result<()> {
    for mark in [STAGING_MARK, PREVIOUS_MARK] {
        for path in leftovers(repos_dir, repository_id, mark).await? {
            tokio::fs::remove_dir_all(path).await?;
        }
    }
    Ok(())
}

/// `name` est-il un reste `.<dépôt><marque><uuid>` de ce dépôt précis ?
fn is_leftover(name: &str, repository_id: &str, mark: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_prefix(repository_id))
        .and_then(|rest| rest.strip_prefix(mark))
        .is_some_and(|unique| uuid::Uuid::try_parse(unique).is_ok())
}

async fn leftovers(
    repos_dir: &Path,
    repository_id: &str,
    mark: &str,
) -> std::io::Result<Vec<PathBuf>> {
    let mut entries = match tokio::fs::read_dir(repos_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| is_leftover(name, repository_id, mark))
        {
            found.push(entry.path());
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = "index.json";

    fn hidden(repository_id: &str, mark: &str) -> String {
        format!(".{repository_id}{mark}{}", uuid::Uuid::new_v4().simple())
    }

    fn copy(dir: &Path, name: &str, content: &str) -> std::io::Result<PathBuf> {
        let path = dir.join(name);
        std::fs::create_dir_all(&path)?;
        std::fs::write(path.join(INDEX), content)?;
        Ok(path)
    }

    fn hidden_entries(dir: &Path) -> std::io::Result<usize> {
        Ok(std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count())
    }

    #[tokio::test]
    async fn test_publish_replaces_the_previous_copy_atomically() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        for content in [r#"{"v":1}"#, r#"{"v":2}"#] {
            let files = [(INDEX.to_owned(), content.as_bytes().to_vec())];
            publish(dir.path(), "repo.test", &files).await?;
        }
        let content = std::fs::read_to_string(dir.path().join("repo.test/index.json"))?;
        assert_eq!(content, r#"{"v":2}"#);
        assert_eq!(hidden_entries(dir.path())?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_publication_interrupted_between_both_renames_is_undone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        copy(dir.path(), &hidden("repo.test", PREVIOUS_MARK), "ancien")?;
        copy(dir.path(), &hidden("repo.test", STAGING_MARK), "nouveau")?;
        assert!(recover(dir.path(), "repo.test").await?);
        let content = std::fs::read_to_string(dir.path().join("repo.test/index.json"))?;
        assert_eq!(content, "ancien");
        assert_eq!(hidden_entries(dir.path())?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_present_copy_is_kept_and_its_leftovers_removed() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        copy(dir.path(), "repo.test", "actuel")?;
        copy(dir.path(), &hidden("repo.test", PREVIOUS_MARK), "ancien")?;
        assert!(!recover(dir.path(), "repo.test").await?);
        let content = std::fs::read_to_string(dir.path().join("repo.test/index.json"))?;
        assert_eq!(content, "actuel");
        assert_eq!(hidden_entries(dir.path())?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn the_leftovers_of_another_repository_are_left_alone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        copy(dir.path(), &hidden("repo.test.autre", PREVIOUS_MARK), "x")?;
        copy(dir.path(), ".repo.test.old-manuel", "x")?;
        assert!(!recover(dir.path(), "repo.test").await?);
        assert!(!dir.path().join("repo.test").exists());
        assert_eq!(hidden_entries(dir.path())?, 2);
        // Dossier des dépôts pas encore créé : rien à reprendre.
        assert!(!recover(&dir.path().join("absent"), "repo.test").await?);
        Ok(())
    }

    #[tokio::test]
    async fn removing_a_copy_also_removes_its_leftovers() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        copy(dir.path(), "repo.test", "actuel")?;
        copy(dir.path(), &hidden("repo.test", PREVIOUS_MARK), "ancien")?;
        let other = copy(dir.path(), "repo.autre", "x")?;
        remove(dir.path(), "repo.test").await?;
        assert!(!dir.path().join("repo.test").exists());
        assert_eq!(hidden_entries(dir.path())?, 0);
        assert!(other.join(INDEX).is_file());
        remove(dir.path(), "repo.test").await?;
        Ok(())
    }
}
