//! Balayage des restes de `/config/installed` : écritures interrompues et copies que la base n'inscrit
//! plus. Une copie remplacée est gardée un temps : un lecteur peut encore la tenir, et une base restaurée
//! l'inscrire de nouveau.

use super::{InstallationGuard, is_storage_dir_name, storage_dir_name};
use crate::{
    error::{AppError, AppResult},
    repository,
};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// Copie en cours d'écriture.
pub(super) const STAGING_PREFIX: &str = ".tmp-";
/// Fichier dont la date est celle où une copie a été vue sans inscription pour la première fois.
const ORPHAN_PREFIX: &str = ".orphan-";
const STAGING_GRACE: Duration = Duration::from_secs(24 * 60 * 60);
const ORPHAN_GRACE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub(super) fn orphan_marker(installed_dir: &Path, name: &str) -> PathBuf {
    installed_dir.join(format!("{ORPHAN_PREFIX}{name}"))
}

/// Retire les restes plus vieux que leur délai de garde et rend leur nombre. Ne touche qu'à ce que
/// Forgearr a écrit. À n'appeler qu'une fois la reprise des dépôts terminée.
pub async fn sweep_orphans(
    _lock: &InstallationGuard,
    pool: &SqlitePool,
    installed_dir: &Path,
) -> AppResult<usize> {
    let kept: HashSet<String> = repository::list_installations(pool)
        .await?
        .iter()
        .filter_map(|item| storage_dir_name(&item.record.item_id, &item.record.content_sha256))
        .collect();
    let installed_dir = installed_dir.to_path_buf();
    let removed =
        tokio::task::spawn_blocking(move || sweep(&installed_dir, &kept, SystemTime::now()))
            .await
            .map_err(|_| AppError::Internal)??;
    Ok(removed)
}

fn sweep(installed_dir: &Path, kept: &HashSet<String>, now: SystemTime) -> io::Result<usize> {
    let entries = match fs::read_dir(installed_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut removed = 0;
    let mut orphans = HashSet::new();
    let mut marked = Vec::new();
    for entry in entries {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if let Some(orphan) = name.strip_prefix(ORPHAN_PREFIX) {
            marked.push(orphan.to_owned());
        } else if !entry.file_type()?.is_dir() {
            continue;
        } else if name.starts_with(STAGING_PREFIX) {
            if age(&entry.path(), now) >= STAGING_GRACE && remove_tree(&entry.path()) {
                removed += 1;
            }
        } else if is_storage_dir_name(&name) && !kept.contains(&name) {
            orphans.insert(name);
        }
    }
    for orphan in &orphans {
        let marker = orphan_marker(installed_dir, orphan);
        if !marker.try_exists()? {
            fs::File::create(&marker)?;
        } else if age(&marker, now) >= ORPHAN_GRACE && remove_tree(&installed_dir.join(orphan)) {
            fs::remove_file(&marker)?;
            removed += 1;
        }
    }
    // Copies inscrites de nouveau, ou disparues.
    for name in marked.iter().filter(|name| !orphans.contains(*name)) {
        fs::remove_file(orphan_marker(installed_dir, name))?;
    }
    Ok(removed)
}

/// Âge d'un chemin ; nul s'il est illisible ou daté du futur, pour ne rien retirer trop tôt.
fn age(path: &Path, now: SystemTime) -> Duration {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .unwrap_or_default()
}

fn remove_tree(path: &Path) -> bool {
    match fs::remove_dir_all(path) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "reste d'installation non effacé");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        opaque_token::sha256_hex,
        plugins::installed::lock_installations,
        repository::{InstallationRecord, save_installation},
    };
    use forgearr_shared::plugins::{store::ItemOrigin, vocab::CatalogItemType};
    use sqlx::sqlite::SqlitePoolOptions;

    fn dir_name(id: &str) -> String {
        format!("{id}.{}", sha256_hex(id))
    }

    #[test]
    fn orphans_and_interrupted_writes_are_removed_after_their_grace() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        let (kept, orphan) = (dir_name("garde"), dir_name("reste"));
        for name in [kept.as_str(), orphan.as_str(), ".tmp-abc", "mes-notes"] {
            fs::create_dir(root.join(name))?;
        }
        let keep = HashSet::from([kept.clone()]);
        let now = SystemTime::now();

        // Premier passage : rien n'est retiré, l'orphelin est daté.
        assert_eq!(sweep(root, &keep, now)?, 0);
        assert!(orphan_marker(root, &orphan).exists());
        assert_eq!(sweep(root, &keep, now + STAGING_GRACE * 2)?, 1);
        assert!(!root.join(".tmp-abc").exists() && root.join(&orphan).exists());
        assert_eq!(sweep(root, &keep, now + ORPHAN_GRACE * 2)?, 1);
        assert!(!root.join(&orphan).exists() && !orphan_marker(root, &orphan).exists());
        // Ni les copies inscrites ni ce que Forgearr n'a pas écrit.
        assert!(root.join(&kept).exists() && root.join("mes-notes").exists());
        assert_eq!(sweep(&root.join("absent"), &keep, now)?, 0);
        Ok(())
    }

    #[test]
    fn a_copy_recorded_again_loses_its_mark() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let name = dir_name("video");
        fs::create_dir(dir.path().join(&name))?;
        let now = SystemTime::now();
        sweep(dir.path(), &HashSet::new(), now)?;
        assert!(orphan_marker(dir.path(), &name).exists());
        let keep = HashSet::from([name.clone()]);
        sweep(dir.path(), &keep, now + ORPHAN_GRACE * 2)?;
        assert!(dir.path().join(&name).exists());
        assert!(!orphan_marker(dir.path(), &name).exists());
        Ok(())
    }

    #[tokio::test]
    async fn the_database_says_what_is_kept() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        let record = InstallationRecord {
            item_id: "video".to_owned(),
            kind: CatalogItemType::Plugin,
            version: "1.0.0".to_owned(),
            content_sha256: sha256_hex("video"),
            origin: ItemOrigin {
                repository_id: "repo.test".to_owned(),
                repository_url: "https://example.org/index.json".to_owned(),
                repository_name: "Test".to_owned(),
            },
            copy_id: None,
            copy_pending: false,
            serves_every_task: false,
        };
        let mut conn = pool.acquire().await?;
        save_installation(&mut conn, &record).await?;
        drop(conn);
        let dir = tempfile::tempdir()?;
        let (kept, orphan) = (dir_name("video"), dir_name("autre"));
        for name in [&kept, &orphan] {
            fs::create_dir(dir.path().join(name))?;
        }
        let lock = lock_installations().await;
        assert_eq!(sweep_orphans(&lock, &pool, dir.path()).await?, 0);
        assert!(!orphan_marker(dir.path(), &kept).exists());
        assert!(orphan_marker(dir.path(), &orphan).exists());
        Ok(())
    }
}
