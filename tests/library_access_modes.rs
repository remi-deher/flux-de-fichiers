//! Modes d'accès des workers aux bibliothèques (ADR 0021) : direct, par transfert, aucun.

use forgearr::{
    domain::{Claim, LibraryAccess, LibraryOutputPolicy, LibraryWorkerPath, LibraryWrite},
    error::AppError,
    repository,
};
use forgearr_shared::workers::INTEGRATED_WORKER_NAME;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

async fn pool() -> anyhow::Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

fn folder(worker_id: Uuid, path: &str, key: &str) -> LibraryWorkerPath {
    LibraryWorkerPath {
        source_key: Some(key.into()),
        worker_id,
        local_path: path.into(),
        access: LibraryAccess::Read,
    }
}

/// Tâche d'une bibliothèque dont la source est montée par `mounts` (worker, chemin).
async fn library_job(
    pool: &sqlx::SqlitePool,
    library: Uuid,
    owner: Option<Uuid>,
    source_key: &str,
    mounts: &[(Uuid, &str)],
) -> anyhow::Result<Uuid> {
    let job = repository::insert_job_with_options(
        pool,
        "film.mkv",
        None,
        &"ab".repeat(32),
        10,
        Some(library),
        None,
    )
    .await?;
    sqlx::query(
        "UPDATE jobs SET source_key=?3, source_root='/mnt/a', source_worker_id=?1 WHERE id=?2",
    )
    .bind(owner)
    .bind(job.id)
    .bind(source_key)
    .execute(pool)
    .await?;
    for (worker, path) in mounts {
        sqlx::query(
            "INSERT INTO job_source_mounts(job_id, worker_id, local_path, writable) VALUES(?1, ?2, ?3, 0)",
        )
        .bind(job.id)
        .bind(*worker)
        .bind(*path)
        .execute(pool)
        .await?;
    }
    Ok(job.id)
}

async fn claim_of(pool: &sqlx::SqlitePool, worker: Uuid) -> anyhow::Result<Option<Claim>> {
    Ok(repository::claim(pool, worker).await?)
}

#[tokio::test]
async fn only_direct_and_transfer_workers_take_library_jobs() -> anyhow::Result<()> {
    let pool = pool().await?;
    let direct = repository::register(&pool, "direct").await?;
    let transfer = repository::register(&pool, "transfert").await?;
    let outsider = repository::register(&pool, "etranger").await?;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            workers: vec![folder(direct.id, "/mnt/a", "nas")],
            transfer_workers: vec![transfer.id],
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(library.transfer_workers, vec![transfer.id]);
    let job = library_job(
        &pool,
        library.id,
        Some(direct.id),
        "nas",
        &[(direct.id, "/mnt/a")],
    )
    .await?;

    assert!(
        claim_of(&pool, outsider.id).await?.is_none(),
        "aucun accès : jamais attribué"
    );
    let claim = claim_of(&pool, transfer.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("le worker par transfert doit obtenir la tâche neuve"))?;
    assert_eq!(claim.job_id, job);
    Ok(())
}

#[tokio::test]
async fn a_direct_worker_keeps_its_jobs_and_the_integrated_worker_serves_central_folders()
-> anyhow::Result<()> {
    let pool = pool().await?;
    let direct = repository::register(&pool, "direct").await?;
    let integrated = repository::register(&pool, INTEGRATED_WORKER_NAME).await?;
    let remote = repository::register(&pool, "distant").await?;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            workers: vec![folder(direct.id, "/mnt/a", "nas")],
            ..Default::default()
        },
    )
    .await?;
    library_job(
        &pool,
        library.id,
        Some(direct.id),
        "nas",
        &[(direct.id, "/mnt/a")],
    )
    .await?;
    assert!(claim_of(&pool, direct.id).await?.is_some());

    // Dossier de la centrale : seul le worker intégré le traite sans être déclaré « par transfert ».
    let central = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Centrale".into(),
            workers: vec![folder(Uuid::nil(), "/data/films", "central")],
            ..Default::default()
        },
    )
    .await?;
    library_job(
        &pool,
        central.id,
        None,
        "central",
        &[(Uuid::nil(), "/data/films")],
    )
    .await?;
    assert!(claim_of(&pool, remote.id).await?.is_none());
    assert!(claim_of(&pool, integrated.id).await?.is_some());
    Ok(())
}

#[tokio::test]
async fn transfer_mode_is_validated_when_saving_a_library() -> anyhow::Result<()> {
    let pool = pool().await?;
    let direct = repository::register(&pool, "direct").await?;
    let other = repository::register(&pool, "autre").await?;
    let save = |transfer: Vec<Uuid>, policy: LibraryOutputPolicy| {
        repository::insert_library(
            &pool,
            LibraryWrite {
                name: format!("Bib {}", Uuid::new_v4()),
                workers: vec![folder(direct.id, "/mnt/a", "nas")],
                transfer_workers: transfer,
                output_policy: policy,
                ..Default::default()
            },
        )
    };
    assert!(
        save(vec![other.id], LibraryOutputPolicy::OutputDir)
            .await
            .is_ok()
    );
    for (transfer, policy) in [
        (vec![direct.id], LibraryOutputPolicy::OutputDir),
        (vec![other.id, other.id], LibraryOutputPolicy::OutputDir),
        (vec![Uuid::nil()], LibraryOutputPolicy::OutputDir),
        (vec![Uuid::new_v4()], LibraryOutputPolicy::OutputDir),
        (vec![other.id], LibraryOutputPolicy::ReplaceInPlace),
        (vec![other.id], LibraryOutputPolicy::ReplaceWithBackup),
    ] {
        assert!(
            matches!(save(transfer.clone(), policy).await, Err(AppError::Invalid)),
            "refusé : {transfer:?} avec {policy:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn transfer_workers_do_not_take_jobs_of_replace_libraries_saved_before() -> anyhow::Result<()>
{
    let pool = pool().await?;
    let direct = repository::register(&pool, "direct").await?;
    let transfer = repository::register(&pool, "transfert").await?;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            workers: vec![folder(direct.id, "/mnt/a", "nas")],
            transfer_workers: vec![transfer.id],
            ..Default::default()
        },
    )
    .await?;
    library_job(
        &pool,
        library.id,
        Some(direct.id),
        "nas",
        &[(direct.id, "/mnt/a")],
    )
    .await?;
    // Politique changée hors de l'API : le claim reste prudent.
    sqlx::query("UPDATE libraries SET output_policy='replace_in_place' WHERE id=?1")
        .bind(library.id)
        .execute(&pool)
        .await?;
    assert!(claim_of(&pool, transfer.id).await?.is_none());
    assert!(claim_of(&pool, direct.id).await?.is_some());
    Ok(())
}

#[tokio::test]
async fn a_second_direct_worker_holds_the_source_when_the_first_is_offline() -> anyhow::Result<()> {
    let pool = pool().await?;
    let first = repository::register(&pool, "nas-un").await?;
    let second = repository::register(&pool, "nas-deux").await?;
    let compute = repository::register(&pool, "calcul").await?;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            workers: vec![
                folder(first.id, "/mnt/a", "nas"),
                folder(second.id, "/srv/films", "nas"),
            ],
            transfer_workers: vec![compute.id],
            ..Default::default()
        },
    )
    .await?;
    let job = library_job(
        &pool,
        library.id,
        Some(first.id),
        "nas",
        &[(first.id, "/mnt/a"), (second.id, "/srv/films")],
    )
    .await?;
    for worker in [first.id, second.id] {
        repository::set_transfer_url(&pool, worker, Some("http://nas.lan:9000")).await?;
    }
    // Le premier détenteur est silencieux depuis longtemps.
    sqlx::query("UPDATE workers SET last_seen=datetime('now', '-1 day') WHERE id=?1")
        .bind(first.id)
        .execute(&pool)
        .await?;
    let holders = repository::source_holders(&pool, job, compute.id).await?;
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].worker_id, second.id);
    assert_eq!(holders[0].root, "/srv/films");
    Ok(())
}

#[test]
fn transfer_worker_rules_are_checked_without_a_database() {
    use forgearr::repository::prepare_transfer_workers;
    let direct = [folder(Uuid::from_u128(1), "/mnt/a", "nas")];
    let other = Uuid::from_u128(2);
    assert!(prepare_transfer_workers(vec![], &direct, LibraryOutputPolicy::ReplaceInPlace).is_ok());
    assert_eq!(
        prepare_transfer_workers(vec![other], &direct, LibraryOutputPolicy::OutputDir).ok(),
        Some(vec![other])
    );
    assert!(
        prepare_transfer_workers(vec![other], &direct, LibraryOutputPolicy::ReplaceInPlace)
            .is_err()
    );
}

#[tokio::test]
async fn the_worker_list_tells_which_workers_announce_a_transfer_address() -> anyhow::Result<()> {
    let pool = pool().await?;
    let announcing = repository::register(&pool, "annonce").await?;
    let silent = repository::register(&pool, "silencieux").await?;
    repository::set_transfer_url(&pool, announcing.id, Some("http://nas.lan:9000")).await?;
    let workers = repository::list_workers(&pool).await?;
    let flag = |id: Uuid| {
        workers
            .iter()
            .find(|worker| worker.id == id)
            .map(|w| w.transfer_enabled)
    };
    assert_eq!(flag(announcing.id), Some(true));
    assert_eq!(flag(silent.id), Some(false));
    repository::set_transfer_url(&pool, announcing.id, None).await?;
    let workers = repository::list_workers(&pool).await?;
    assert!(workers.iter().all(|worker| !worker.transfer_enabled));
    Ok(())
}

#[tokio::test]
async fn publication_selects_only_current_writable_online_holders() -> anyhow::Result<()> {
    let pool = pool().await?;
    let reader = repository::register(&pool, "lecteur").await?;
    let writer = repository::register(&pool, "ecrivain").await?;
    let compute = repository::register(&pool, "calcul").await?;
    let mut writable = folder(writer.id, "/srv/films", "nas");
    writable.access = LibraryAccess::ReadWrite;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            workers: vec![folder(reader.id, "/mnt/a", "nas"), writable],
            transfer_workers: vec![compute.id],
            ..Default::default()
        },
    )
    .await?;
    let job = library_job(
        &pool,
        library.id,
        Some(reader.id),
        "nas",
        &[(reader.id, "/mnt/a"), (writer.id, "/srv/films")],
    )
    .await?;
    let holders = repository::writable_source_holders(&pool, job, compute.id).await?;
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].worker_id, writer.id);
    sqlx::query("UPDATE workers SET last_seen=datetime('now', '-1 day') WHERE id=?1")
        .bind(writer.id)
        .execute(&pool)
        .await?;
    assert!(
        repository::writable_source_holders(&pool, job, compute.id)
            .await?
            .is_empty()
    );
    sqlx::query("UPDATE workers SET last_seen=datetime('now') WHERE id=?1")
        .bind(writer.id)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE library_folders SET access='read' WHERE worker_id=?1")
        .bind(writer.id)
        .execute(&pool)
        .await?;
    assert!(
        repository::writable_source_holders(&pool, job, compute.id)
            .await?
            .is_empty()
    );
    Ok(())
}
