//! Classe de ressource (CPU/GPU) d'une tâche : sous-flux et flux d'échec figés comptent comme
//! le flux principal (ADR 0026).

use forgearr::{
    domain::{LibraryWrite, UpdateFlowRequest},
    repository,
};
use forgearr_shared::{flows::MAX_SUB_FLOW_DEPTH, jobs::JobResourceClass};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

const MAIN_FLOW: &str = "flow.principal";
const FAILURE_FLOW: &str = "flow.echec";
const GPU_ACCELERATION: &str = "nvenc";
const CPU_ACCELERATION: &str = "cpu";

async fn pool() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Bloc d'encodage avec l'accélération indiquée.
fn encoder(acceleration: &str) -> Value {
    json!({"id": "enc", "type": "video.encode", "name": "Encoder",
           "config": {"codec": "hevc", "acceleration": acceleration}})
}

/// Graphe CPU qui appelle `target` en sous-flux.
fn calling(target: &str) -> Value {
    json!({
        "nodes": [
            encoder(CPU_ACCELERATION),
            {"id": "call", "type": "essentiel.sub-flow", "name": "Sous-flux",
             "config": {"flowId": target}}
        ],
        "connections": [{"from": "enc", "to": "call"}]
    })
}

fn encoding(acceleration: &str) -> Value {
    json!({"nodes": [encoder(acceleration)], "connections": []})
}

async fn save(pool: &SqlitePool, id: &str, definition: &Value) -> anyhow::Result<()> {
    let request = UpdateFlowRequest {
        name: id.into(),
        description: None,
        file_mask: "*.mkv".into(),
        is_default: false,
        definition: definition.to_string(),
    };
    repository::save_flow(pool, id, &request).await?;
    Ok(())
}

async fn class_of_new_job(pool: &SqlitePool, flow: &str) -> anyhow::Result<JobResourceClass> {
    let job = repository::insert_job_with_priority(
        pool,
        "film.mkv",
        None,
        &"ab".repeat(32),
        1_000,
        None,
        Some(flow),
        None,
    )
    .await?;
    Ok(repository::job_resource_class(pool, job.id).await?)
}

/// Chaîne `MAIN_FLOW → f1 → … → f{depth}` ; le dernier maillon encode avec `acceleration`.
async fn save_chain(pool: &SqlitePool, depth: usize, acceleration: &str) -> anyhow::Result<()> {
    let link = |level: usize| format!("flow.niveau{level}");
    save(pool, &link(depth), &encoding(acceleration)).await?;
    for level in (1..depth).rev() {
        save(pool, &link(level), &calling(&link(level + 1))).await?;
    }
    save(pool, MAIN_FLOW, &calling(&link(1))).await
}

#[tokio::test]
async fn a_gpu_sub_flow_makes_the_job_gpu() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_chain(&pool, MAX_SUB_FLOW_DEPTH, GPU_ACCELERATION).await?;
    assert_eq!(
        class_of_new_job(&pool, MAIN_FLOW).await?,
        JobResourceClass::Gpu
    );
    Ok(())
}

#[tokio::test]
async fn sub_flows_beyond_the_maximum_depth_are_neither_frozen_nor_classified() -> anyhow::Result<()>
{
    let pool = pool().await?;
    save_chain(&pool, MAX_SUB_FLOW_DEPTH + 1, GPU_ACCELERATION).await?;
    assert_eq!(
        class_of_new_job(&pool, MAIN_FLOW).await?,
        JobResourceClass::Cpu
    );
    let frozen: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_linked_flows WHERE role='sub_flow'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(usize::try_from(frozen)?, MAX_SUB_FLOW_DEPTH);
    Ok(())
}

#[tokio::test]
async fn a_gpu_failure_flow_makes_the_job_gpu() -> anyhow::Result<()> {
    let pool = pool().await?;
    save(&pool, FAILURE_FLOW, &encoding(GPU_ACCELERATION)).await?;
    let mut main = encoding(CPU_ACCELERATION);
    main["failureFlowId"] = json!(FAILURE_FLOW);
    save(&pool, MAIN_FLOW, &main).await?;
    assert_eq!(
        class_of_new_job(&pool, MAIN_FLOW).await?,
        JobResourceClass::Gpu
    );
    Ok(())
}

#[tokio::test]
async fn cpu_linked_flows_keep_the_job_cpu() -> anyhow::Result<()> {
    let pool = pool().await?;
    save_chain(&pool, 1, CPU_ACCELERATION).await?;
    assert_eq!(
        class_of_new_job(&pool, MAIN_FLOW).await?,
        JobResourceClass::Cpu
    );
    Ok(())
}

/// Relance d'une version antérieure qui appelait un sous-flux GPU, la dernière n'en appelant pas.
#[tokio::test]
async fn reprocess_classifies_the_sub_flows_of_the_replayed_version() -> anyhow::Result<()> {
    const CALLING_VERSION: i64 = 1;
    let pool = pool().await?;
    save_chain(&pool, 1, GPU_ACCELERATION).await?;
    save(&pool, MAIN_FLOW, &encoding(CPU_ACCELERATION)).await?;
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
        Some(MAIN_FLOW),
        None,
    )
    .await?;
    assert_eq!(
        repository::job_resource_class(&pool, previous.id).await?,
        JobResourceClass::Cpu
    );
    sqlx::query("UPDATE jobs SET status='failed' WHERE id=?")
        .bind(previous.id)
        .execute(&pool)
        .await?;
    let replay: Uuid = repository::library_history::reprocess(
        &pool,
        library.id,
        previous.id,
        Some(CALLING_VERSION),
    )
    .await?
    .id;
    assert_eq!(
        repository::job_resource_class(&pool, replay).await?,
        JobResourceClass::Gpu
    );
    Ok(())
}
