//! Fichiers retenus chez un worker en attendant que le preneur les tire (ADR 0022) : noms, identité
//! et purge de ceux que le central ne réclame plus. Partagé par le worker distant et le worker
//! intégré.

use crate::storage;
use forgearr_shared::{relay::HANDOFF_TTL, transfer::ArtifactKind};
use std::{
    path::Path,
    time::{Duration, SystemTime},
};
use uuid::Uuid;

/// Préfixe du nom d'un fichier retenu, selon sa nature.
const fn retained_prefix(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Working => "handoff-",
        ArtifactKind::Result => "result-",
    }
}

/// Nom du fichier retenu : préfixe de sa nature, `<tâche>-<époque>`, puis l'extension du fichier
/// d'origine, que les sondes et les blocs du flux lisent.
pub(crate) fn retained_name(kind: ArtifactKind, job: Uuid, epoch: i64, file: &Path) -> String {
    let base = format!("{}{job}-{epoch}", retained_prefix(kind));
    let extension = file.extension().and_then(|ext| ext.to_str());
    match extension.filter(|ext| storage::valid_output_extension(ext)) {
        Some(extension) => format!("{base}.{extension}"),
        None => base,
    }
}

fn retained_identity(name: &str) -> Option<(ArtifactKind, Uuid, i64)> {
    [ArtifactKind::Working, ArtifactKind::Result]
        .into_iter()
        .find_map(|kind| {
            let stem = name
                .strip_prefix(retained_prefix(kind))?
                .split('.')
                .next()?;
            let (job, epoch) = stem.rsplit_once('-')?;
            let epoch = epoch.parse::<i64>().ok().filter(|epoch| *epoch >= 0)?;
            Some((kind, Uuid::parse_str(job).ok()?, epoch))
        })
}

/// Ne retire une copie ancienne qu'après confirmation de son inutilité par le central.
pub(crate) async fn prune_retained<F, Fut>(root: &Path, needed: F) -> anyhow::Result<()>
where
    F: Fn(ArtifactKind, Uuid, i64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<bool>>,
{
    prune_retained_older_than(root, HANDOFF_TTL, needed).await
}

async fn prune_retained_older_than<F, Fut>(
    root: &Path,
    max_age: Duration,
    needed: F,
) -> anyhow::Result<()>
where
    F: Fn(ArtifactKind, Uuid, i64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<bool>>,
{
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some((kind, job, epoch)) = retained_identity(&name) else {
            continue;
        };
        let modified = entry.metadata().await?.modified()?;
        if SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default()
            < max_age
        {
            continue;
        }
        if !needed(kind, job, epoch).await? {
            match tokio::fs::remove_file(entry.path()).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(job_id=%job, error=%error, "retained handoff cleanup failed")
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_names_are_scoped_to_kind_job_and_lease_and_keep_the_extension() {
        let job = Uuid::nil();
        let work = ArtifactKind::Working;
        let with_extension = Path::new("out.mkv");
        assert_eq!(
            retained_name(work, job, 7, with_extension),
            format!("handoff-{job}-7.mkv")
        );
        assert_eq!(
            retained_name(ArtifactKind::Result, job, 7, with_extension),
            format!("result-{job}-7.mkv")
        );
        assert_eq!(
            retained_name(work, job, 7, Path::new("out")),
            format!("handoff-{job}-7")
        );
        assert_ne!(
            retained_name(work, job, 7, with_extension),
            retained_name(work, job, 8, with_extension)
        );
        // Une extension qui n'en est pas une n'entre pas dans le nom.
        let odd = Path::new("out.m v");
        assert_eq!(retained_name(work, job, 7, odd), format!("handoff-{job}-7"));
    }

    #[test]
    fn retained_names_give_back_their_kind_job_and_lease() {
        let job = Uuid::nil();
        let work = ArtifactKind::Working;
        assert_eq!(
            retained_identity(&format!("handoff-{job}-7")),
            Some((work, job, 7))
        );
        assert_eq!(
            retained_identity(&format!("handoff-{job}-7.mkv")),
            Some((work, job, 7))
        );
        assert_eq!(
            retained_identity(&format!("result-{job}-9.mkv")),
            Some((ArtifactKind::Result, job, 9))
        );
        assert!(retained_identity("handoff-not-a-uuid-7").is_none());
        assert!(retained_identity(&format!("handoff-{job}--1")).is_none());
        assert!(retained_identity("autre-fichier.mkv").is_none());
    }

    #[tokio::test]
    async fn cleanup_preserves_needed_or_uncertain_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let kind = ArtifactKind::Result;
        let name = retained_name(kind, Uuid::new_v4(), 1, Path::new("w.mkv"));
        let retained = dir.path().join(name);
        tokio::fs::write(&retained, b"data").await?;
        prune_retained_older_than(dir.path(), Duration::ZERO, |_, _, _| async { Ok(true) }).await?;
        assert!(retained.exists());
        assert!(
            prune_retained_older_than(dir.path(), Duration::ZERO, |_, _, _| async {
                Err(anyhow::anyhow!("offline"))
            })
            .await
            .is_err()
        );
        assert!(retained.exists());
        prune_retained_older_than(dir.path(), Duration::ZERO, |_, _, _| async { Ok(false) })
            .await?;
        assert!(!retained.exists());
        Ok(())
    }
}
