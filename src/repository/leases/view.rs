//! État des baux pour l'interface.

use crate::{domain::JobStatus, error::AppResult};
use chrono::{DateTime, Utc};
use forgearr_shared::{
    jobs::{JobLeaseInfo, LeaseReachability},
    workers::WORKER_ONLINE_SECONDS,
};
use sqlx::SqlitePool;
use uuid::Uuid;

/// État des baux des tâches indiquées : en cours, ou en file pour un worker désigné hors ligne.
pub async fn lease_infos(pool: &SqlitePool, ids: &[Uuid]) -> AppResult<Vec<JobLeaseInfo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT j.id, j.status, j.unreachable_since, j.leased_until FROM jobs j \
         LEFT JOIN workers w ON w.id=j.target_worker_id \
         WHERE (j.status=? OR (j.status=? AND w.last_seen <= datetime('now', '-' || ? || ' seconds'))) \
         AND j.id IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<
        _,
        (
            Uuid,
            JobStatus,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
        ),
    >(&sql)
    .bind(JobStatus::Running)
    .bind(JobStatus::Queued)
    .bind(WORKER_ONLINE_SECONDS);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(job_id, status, unreachable_since, expires_at)| {
            let running = status == JobStatus::Running;
            let reachability = match (running, unreachable_since) {
                (false, _) => LeaseReachability::AwaitingWorker,
                (true, Some(_)) => LeaseReachability::Unreachable,
                (true, None) => LeaseReachability::Reachable,
            };
            JobLeaseInfo {
                job_id,
                reachability,
                unreachable_since: unreachable_since.filter(|_| running),
                expires_at: expires_at.filter(|_| running),
            }
        })
        .collect())
}
