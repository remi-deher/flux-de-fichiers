//! Purge, au démarrage seulement, de ce qu'un processus précédent a laissé dans le dossier de travail.
//! À la reconnexion, elle détruirait le travail continué pendant la coupure (ADR 0027).

use std::path::Path;
use uuid::Uuid;

/// Longueur d'un UUID écrit avec ses tirets.
const UUID_LENGTH: usize = 36;

/// Dossier `<tâche>-<bail>` d'une exécution.
fn is_task_dir(name: &str) -> bool {
    let (Some(job), Some(rest)) = (name.get(..UUID_LENGTH), name.get(UUID_LENGTH..)) else {
        return false;
    };
    let Some(token) = rest.strip_prefix('-') else {
        return false;
    };
    token.len() == UUID_LENGTH && Uuid::parse_str(job).is_ok() && Uuid::parse_str(token).is_ok()
}

fn is_leftover(name: &str, is_dir: bool) -> bool {
    name.starts_with("job-")
        || name.starts_with(".tmp")
        || name.ends_with(".tmp")
        || (is_dir && is_task_dir(name))
}

/// Supprime les résidus sans toucher aux outils, à la configuration ni aux fichiers retenus.
pub(super) async fn purge(work_dir: &Path) {
    purge_matching(work_dir, is_leftover).await;
}

/// Supprime seulement les dossiers de tâches (dossier de travail du worker intégré).
pub(crate) async fn purge_task_dirs(work_dir: &Path) {
    purge_matching(work_dir, |name, is_dir| is_dir && is_task_dir(name)).await;
}

/// Supprime les entrées dont le nom et la nature (dossier ou non) sont retenus.
pub(crate) async fn purge_matching(work_dir: &Path, matches: impl Fn(&str, bool) -> bool) {
    let Ok(mut entries) = tokio::fs::read_dir(work_dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let is_dir = entry.file_type().await.is_ok_and(|kind| kind.is_dir());
        if !matches(&entry.file_name().to_string_lossy(), is_dir) {
            continue;
        }
        let path = entry.path();
        tracing::info!(path = %path.display(), "purging orphaned work artifact");
        let removed = if is_dir {
            tokio::fs::remove_dir_all(&path).await
        } else {
            tokio::fs::remove_file(&path).await
        };
        if let Err(error) = removed {
            tracing::warn!(path = %path.display(), %error, "orphaned work artifact not purged");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_the_leftovers_of_a_previous_run_are_removed() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (job, token) = (Uuid::new_v4(), Uuid::new_v4());
        let task = dir.path().join(format!("{job}-{token}"));
        let retained = dir.path().join(format!("result-{job}-3.mkv"));
        let tools = dir.path().join("tools-cache");
        tokio::fs::create_dir(&task).await?;
        tokio::fs::create_dir(&tools).await?;
        tokio::fs::write(task.join("film.mkv"), b"x").await?;
        tokio::fs::write(&retained, b"x").await?;
        let temporary = dir.path().join(".source-publication-1.tmp");
        tokio::fs::write(&temporary, b"x").await?;
        purge(dir.path()).await;
        assert!(!task.exists() && !temporary.exists());
        assert!(retained.exists() && tools.exists());
        assert!(!is_task_dir(&job.to_string()));
        assert!(!is_task_dir(
            "é-dossier-quelconque-de-plus-de-trente-six-octets"
        ));
        Ok(())
    }

    #[tokio::test]
    async fn the_integrated_worker_leaves_failure_flows_alone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let task = format!("{}-{}", Uuid::new_v4(), Uuid::new_v4());
        let flow = format!("failure-flow-{}", Uuid::new_v4());
        for name in [&task, &flow] {
            tokio::fs::create_dir(dir.path().join(name)).await?;
        }
        purge_task_dirs(dir.path()).await;
        assert!(!dir.path().join(task).exists());
        assert!(dir.path().join(flow).exists());
        Ok(())
    }
}
