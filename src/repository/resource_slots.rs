//! Places CPU/GPU des workers et classe de ressource des tâches.
//!
//! Requêtes sous forme runtime (le cache SQLx hors ligne ne connaît pas ces colonnes) : les
//! macros existantes qui lisent `workers` ou `jobs` restent inchangées et ces fonctions
//! complètent leurs résultats.

use crate::{
    domain::{JobStatus, Worker},
    error::AppResult,
};
use forgearr_shared::flows::{FlowGraph, flow_resource_class};
use forgearr_shared::jobs::{JobResourceClass, JobResourceInfo};
use forgearr_shared::workers::RunningJobs;
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;
use uuid::Uuid;

/// Colonnes d'un worker absentes des macros `query_as!(WorkerRow, …)`.
#[derive(sqlx::FromRow)]
pub(super) struct WorkerExtras {
    id: Uuid,
    transfer_enabled: bool,
    max_cpu_jobs: Option<i32>,
    max_gpu_jobs: Option<i32>,
}

const WORKER_EXTRAS_SQL: &str = "SELECT id, transfer_url IS NOT NULL AS transfer_enabled, max_cpu_jobs, max_gpu_jobs FROM workers";

impl WorkerExtras {
    pub(super) fn apply(self, worker: &mut Worker) {
        worker.transfer_enabled = self.transfer_enabled;
        worker.max_cpu_jobs = self.max_cpu_jobs;
        worker.max_gpu_jobs = self.max_gpu_jobs;
    }
}

/// Complète un worker lu par macro (transfert annoncé, places par classe).
pub(super) async fn with_extras(pool: &SqlitePool, mut worker: Worker) -> AppResult<Worker> {
    let extras = sqlx::query_as::<_, WorkerExtras>(&format!("{WORKER_EXTRAS_SQL} WHERE id=?1"))
        .bind(worker.id)
        .fetch_one(pool)
        .await?;
    extras.apply(&mut worker);
    Ok(worker)
}

/// Compléments de tous les workers, par identifiant (listes).
pub(super) async fn all_extras(pool: &SqlitePool) -> AppResult<HashMap<Uuid, WorkerExtras>> {
    Ok(sqlx::query_as::<_, WorkerExtras>(WORKER_EXTRAS_SQL)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|extras| (extras.id, extras))
        .collect())
}

/// Enregistre les places par classe (`None` des deux côtés : mode historique), déjà validées
/// par `class_slots_valid`.
pub(super) async fn save_class_slots(
    conn: &mut SqliteConnection,
    worker: Uuid,
    max_cpu_jobs: Option<i32>,
    max_gpu_jobs: Option<i32>,
) -> AppResult<()> {
    sqlx::query("UPDATE workers SET max_cpu_jobs=?2, max_gpu_jobs=?3 WHERE id=?1")
        .bind(worker)
        .bind(max_cpu_jobs)
        .bind(max_gpu_jobs)
        .execute(conn)
        .await?;
    Ok(())
}

/// Tâches en cours (bail valide) d'un worker, par classe.
pub(super) async fn running_jobs(
    conn: &mut SqliteConnection,
    worker: Uuid,
) -> AppResult<RunningJobs> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT resource_class, COUNT(*) FROM jobs
         WHERE worker_id=?1 AND status=?2 AND leased_until > datetime('now')
         GROUP BY resource_class",
    )
    .bind(worker)
    .bind(JobStatus::Running)
    .fetch_all(conn)
    .await?;
    let mut running = RunningJobs::default();
    for (class, count) in rows {
        running.add(JobResourceClass::parse_or_default(&class), count);
    }
    Ok(running)
}

/// Classe de ressource du flux figé `flow_id@version` ; sans flux lisible : CPU.
pub(super) async fn frozen_flow_class(
    conn: &mut SqliteConnection,
    flow: Option<(&str, i64)>,
) -> AppResult<JobResourceClass> {
    let Some((flow_id, version)) = flow else {
        return Ok(JobResourceClass::Cpu);
    };
    let definition: Option<String> =
        sqlx::query_scalar("SELECT definition FROM flow_versions WHERE flow_id=?1 AND version=?2")
            .bind(flow_id)
            .bind(version)
            .fetch_optional(conn)
            .await?;
    Ok(definition
        .map(|definition| flow_resource_class(&FlowGraph::parse(&definition)))
        .unwrap_or_default())
}

/// Fige la classe de ressource d'une tâche qui vient d'être créée.
pub(super) async fn set_job_class(
    conn: &mut SqliteConnection,
    job: Uuid,
    class: JobResourceClass,
) -> AppResult<()> {
    sqlx::query("UPDATE jobs SET resource_class=?2 WHERE id=?1")
        .bind(job)
        .bind(class.as_str())
        .execute(conn)
        .await?;
    Ok(())
}

/// Classe de ressource d'une tâche (CPU si la tâche n'existe pas).
pub async fn job_resource_class(pool: &SqlitePool, job: Uuid) -> AppResult<JobResourceClass> {
    let class: Option<String> = sqlx::query_scalar("SELECT resource_class FROM jobs WHERE id=?1")
        .bind(job)
        .fetch_optional(pool)
        .await?;
    Ok(class
        .as_deref()
        .map(JobResourceClass::parse_or_default)
        .unwrap_or_default())
}

/// Classes de ressource des tâches indiquées (identifiants inconnus ignorés).
pub async fn job_resource_classes(
    pool: &SqlitePool,
    ids: &[Uuid],
) -> AppResult<Vec<JobResourceInfo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT id, resource_class FROM jobs WHERE id IN ({placeholders})");
    let mut query = sqlx::query_as::<_, (Uuid, String)>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(job_id, class)| JobResourceInfo {
            job_id,
            resource_class: JobResourceClass::parse_or_default(&class),
        })
        .collect())
}
