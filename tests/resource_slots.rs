//! Places CPU/GPU des workers, classe de ressource des tâches et priorité des bibliothèques.

use forgearr::{
    domain::{LibraryWrite, UpdateFlowRequest},
    error::AppError,
    repository,
};
use forgearr_shared::jobs::JobResourceClass;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

const CPU_FLOW: &str = "flow.cpu";
const GPU_FLOW: &str = "flow.gpu";

async fn pool() -> anyhow::Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

fn encode_flow(acceleration: &str) -> String {
    serde_json::json!({
        "nodes": [{"id": "enc", "type": "video.encode", "name": "Encoder",
                   "config": {"codec": "hevc", "acceleration": acceleration}}],
        "connections": []
    })
    .to_string()
}

/// Flux CPU (par défaut) et flux GPU (NVENC).
async fn save_flows(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    for (id, acceleration, is_default) in [(CPU_FLOW, "cpu", true), (GPU_FLOW, "nvenc", false)] {
        repository::save_flow(
            pool,
            id,
            &UpdateFlowRequest {
                name: id.into(),
                description: None,
                file_mask: "*.mkv".into(),
                is_default,
                definition: encode_flow(acceleration),
            },
        )
        .await?;
    }
    Ok(())
}

/// Tâche hors bibliothèque sur le flux indiqué ; `seed` distingue les empreintes.
async fn job(
    pool: &sqlx::SqlitePool,
    flow: &str,
    seed: u8,
    priority: Option<i32>,
) -> anyhow::Result<Uuid> {
    Ok(repository::insert_job_with_priority(
        pool,
        &format!("{flow}-{seed}.mkv"),
        None,
        &format!("{seed:02x}").repeat(32),
        1_000,
        None,
        Some(flow),
        priority,
    )
    .await?
    .id)
}

/// Priorité d'une tâche créée dans `library`.
async fn library_job(
    pool: &sqlx::SqlitePool,
    library: Uuid,
    seed: u8,
    priority: Option<i32>,
) -> anyhow::Result<i32> {
    Ok(repository::insert_job_with_priority(
        pool,
        &format!("film-{seed}.mkv"),
        None,
        &format!("{seed:02x}").repeat(32),
        1,
        Some(library),
        None,
        priority,
    )
    .await?
    .priority)
}

async fn claimed(pool: &sqlx::SqlitePool, worker: Uuid) -> anyhow::Result<Option<Uuid>> {
    Ok(repository::claim(pool, worker)
        .await?
        .map(|claim| claim.job_id))
}

#[tokio::test]
async fn migration_defaults_keep_the_legacy_behaviour() -> anyhow::Result<()> {
    let pool = pool().await?;
    let worker = repository::register(&pool, "legacy").await?;
    assert_eq!((worker.max_cpu_jobs, worker.max_gpu_jobs), (None, None));
    let job = repository::insert_job(&pool, "a.mkv", None, &"aa".repeat(32), 1).await?;
    assert_eq!(
        repository::job_resource_class(&pool, job.id).await?,
        JobResourceClass::Cpu
    );
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(library.default_priority, 0);
    Ok(())
}

#[tokio::test]
async fn gpu_flow_job_gets_gpu_resource_class() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_flows(&pool).await?;
    let cpu = job(&pool, CPU_FLOW, 1, None).await?;
    let gpu = job(&pool, GPU_FLOW, 2, None).await?;
    assert_eq!(
        repository::job_resource_class(&pool, gpu).await?,
        JobResourceClass::Gpu
    );
    let mut listed = repository::job_resource_classes(&pool, &[cpu, gpu, Uuid::nil()]).await?;
    listed.sort_by_key(|entry| entry.job_id == gpu);
    assert_eq!(
        listed
            .iter()
            .map(|entry| (entry.job_id, entry.resource_class))
            .collect::<Vec<_>>(),
        vec![(cpu, JobResourceClass::Cpu), (gpu, JobResourceClass::Gpu)]
    );
    assert!(
        repository::job_resource_classes(&pool, &[])
            .await?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn job_inherits_library_default_priority() -> anyhow::Result<()> {
    let pool = pool().await?;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Urgent".into(),
            default_priority: 20,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(library.default_priority, 20);
    assert_eq!(
        repository::get_library(&pool, library.id)
            .await?
            .default_priority,
        20
    );
    assert_eq!(library_job(&pool, library.id, 1, None).await?, 20);
    assert_eq!(library_job(&pool, library.id, 2, Some(-5)).await?, -5);

    let out_of_range = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Trop".into(),
            default_priority: 101,
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(out_of_range, Err(AppError::Invalid)));
    let updated = repository::update_library(
        &pool,
        library.id,
        LibraryWrite {
            name: "Urgent".into(),
            default_priority: -3,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(updated.default_priority, -3);
    let listed = repository::list_libraries(&pool).await?;
    assert_eq!(listed.first().map(|lib| lib.default_priority), Some(-3));
    Ok(())
}

#[tokio::test]
async fn claim_respects_priority_before_age() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_flows(&pool).await?;
    let worker = repository::register(&pool, "prio").await?;
    let older = job(&pool, CPU_FLOW, 1, Some(0)).await?;
    let newer = job(&pool, CPU_FLOW, 2, Some(10)).await?;
    repository::update_worker_settings(&pool, worker.id, 2, None, None, None).await?;
    assert_eq!(claimed(&pool, worker.id).await?, Some(newer));
    assert_eq!(claimed(&pool, worker.id).await?, Some(older));
    Ok(())
}

#[tokio::test]
async fn legacy_worker_without_slots_shares_max_concurrency() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_flows(&pool).await?;
    let worker = repository::register(&pool, "legacy").await?;
    let gpu = job(&pool, GPU_FLOW, 1, Some(10)).await?;
    job(&pool, CPU_FLOW, 2, None).await?;
    assert_eq!(claimed(&pool, worker.id).await?, Some(gpu));
    assert_eq!(claimed(&pool, worker.id).await?, None);
    Ok(())
}

#[tokio::test]
async fn separate_slots_allow_cpu_while_gpu_runs() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_flows(&pool).await?;
    let worker = repository::register(&pool, "split").await?;
    let updated =
        repository::update_worker_settings(&pool, worker.id, 1, Some(1), Some(1), None).await?;
    assert_eq!(
        (updated.max_cpu_jobs, updated.max_gpu_jobs),
        (Some(1), Some(1))
    );
    assert_eq!(updated.slots().total, 2);
    let gpu = job(&pool, GPU_FLOW, 1, Some(10)).await?;
    job(&pool, GPU_FLOW, 2, Some(5)).await?;
    let cpu = job(&pool, CPU_FLOW, 3, None).await?;
    assert_eq!(claimed(&pool, worker.id).await?, Some(gpu));
    // La seconde tâche GPU, plus prioritaire, attend : seule la place CPU est libre.
    assert_eq!(claimed(&pool, worker.id).await?, Some(cpu));
    assert_eq!(claimed(&pool, worker.id).await?, None);
    Ok(())
}

#[tokio::test]
async fn gpu_slots_zero_skips_gpu_jobs_but_claims_cpu() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_flows(&pool).await?;
    let worker = repository::register(&pool, "cpu-only").await?;
    repository::update_worker_settings(&pool, worker.id, 1, Some(1), Some(0), None).await?;
    job(&pool, GPU_FLOW, 1, Some(50)).await?;
    let cpu = job(&pool, CPU_FLOW, 2, None).await?;
    assert_eq!(claimed(&pool, worker.id).await?, Some(cpu));
    assert_eq!(claimed(&pool, worker.id).await?, None);
    Ok(())
}

#[tokio::test]
async fn update_worker_settings_validates_class_slots() -> anyhow::Result<()> {
    let pool = pool().await?;
    let worker = repository::register(&pool, "bounds").await?;
    for (cpu, gpu) in [
        (Some(0), Some(0)),
        (Some(17), None),
        (None, Some(-1)),
        (Some(0), None),
    ] {
        assert!(matches!(
            repository::update_worker_settings(&pool, worker.id, 1, cpu, gpu, None).await,
            Err(AppError::Invalid)
        ));
    }
    repository::update_worker_settings(&pool, worker.id, 3, None, Some(2), None).await?;
    let listed = repository::list_workers(&pool).await?;
    assert_eq!(
        listed.first().map(|w| (w.max_cpu_jobs, w.max_gpu_jobs)),
        Some((None, Some(2)))
    );
    let reset = repository::update_worker_settings(&pool, worker.id, 3, None, None, None).await?;
    assert_eq!((reset.max_cpu_jobs, reset.max_gpu_jobs), (None, None));
    assert_eq!(
        repository::get_worker(&pool, worker.id)
            .await?
            .slots()
            .total,
        3
    );
    Ok(())
}

/// Relance d'une version GPU antérieure alors que la dernière version du flux est CPU.
#[tokio::test]
async fn reprocess_of_an_older_gpu_version_keeps_the_gpu_class() -> anyhow::Result<()> {
    const MIXED_FLOW: &str = "flow.mixte";
    const GPU_VERSION: i64 = 1;
    let pool = pool().await?;
    for acceleration in ["nvenc", "cpu"] {
        repository::save_flow(
            &pool,
            MIXED_FLOW,
            &UpdateFlowRequest {
                name: MIXED_FLOW.into(),
                description: None,
                file_mask: "*.mkv".into(),
                is_default: false,
                definition: encode_flow(acceleration),
            },
        )
        .await?;
    }
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            ..Default::default()
        },
    )
    .await?;
    let previous = repository::insert_job_with_priority(
        &pool,
        "film.mkv",
        None,
        &"cd".repeat(32),
        1_000,
        Some(library.id),
        Some(MIXED_FLOW),
        None,
    )
    .await?;
    assert_eq!(
        repository::job_resource_class(&pool, previous.id).await?,
        JobResourceClass::Cpu,
        "la dernière version est CPU"
    );
    sqlx::query("UPDATE jobs SET status='failed' WHERE id=?")
        .bind(previous.id)
        .execute(&pool)
        .await?;
    let replay =
        repository::library_history::reprocess(&pool, library.id, previous.id, Some(GPU_VERSION))
            .await?;
    assert_eq!(replay.flow_version, Some(GPU_VERSION));
    assert_eq!(
        repository::job_resource_class(&pool, replay.id).await?,
        JobResourceClass::Gpu
    );
    Ok(())
}
