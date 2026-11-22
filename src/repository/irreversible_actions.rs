//! Registre des actions irréversibles autorisées sous un bail (ADR 0027).
//!
//! L'inscription précède toujours l'action : un worker mort entre les deux donne un échec par excès
//! de prudence, jamais une action répétée.

use super::{leases, source_publication_journal as journal};
use crate::{
    domain::{AuthorizeAction, IrreversibleAction, JobErrorCode},
    error::{AppError, AppResult},
    job_runner::FAILURE_FLOW_STEP_NODE,
};
use forgearr_shared::jobs::ACTION_NODE_ID_MAX_CHARS;
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Action qui exclut la remise en file automatique de la tâche.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct BlockingAction {
    pub action: IrreversibleAction,
    pub node_id: String,
    pub error_code: Option<JobErrorCode>,
}

impl BlockingAction {
    pub(super) fn refusal_message(&self) -> String {
        format!(
            "reprise automatique exclue : action irréversible {} autorisée (bloc {})",
            self.action.as_str(),
            self.node_id
        )
    }
}

/// Nœud inscrit au registre : le flux d'échec n'a qu'un accord par bail, sous un nœud réservé.
fn recorded_node(request: &AuthorizeAction) -> AppResult<&str> {
    let node = match request.action {
        IrreversibleAction::FailureFlow => FAILURE_FLOW_STEP_NODE,
        IrreversibleAction::InPlace | IrreversibleAction::HttpCall => request.node_id.as_str(),
    };
    if node.is_empty() || node.contains('\0') || node.chars().count() > ACTION_NODE_ID_MAX_CHARS {
        return Err(AppError::Invalid);
    }
    Ok(node)
}

/// Donne au titulaire l'accord d'exécuter l'action et l'inscrit dans la même transaction. Un accord
/// redemandé par le même bail est redonné : sa réponse s'était perdue, l'action n'a pas commencé.
///
/// `LeaseLost` : l'appelant n'est plus titulaire. `Conflict` : le bail vaut, mais l'action est
/// exclue (publication en cours, flux d'échec déjà autorisé).
pub async fn authorize_action(
    pool: &SqlitePool,
    job: Uuid,
    request: &AuthorizeAction,
) -> AppResult<()> {
    let node = recorded_node(request)?;
    let failure_flow = request.action == IrreversibleAction::FailureFlow;
    let reportable = request
        .error_code
        .is_some_and(JobErrorCode::is_worker_reportable);
    if failure_flow && !reportable {
        return Err(AppError::Invalid);
    }
    let timeout = crate::runtime_settings::load_unreachable_timeout(pool).await;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    leases::ensure_held(&mut tx, job, &request.lease()).await?;
    leases::refresh_contact(&mut tx, job, timeout).await?;
    if failure_flow {
        take_failure_flow(&mut tx, job, request.lease_epoch).await?;
    } else {
        ensure_no_failure_flow(&mut tx, job).await?;
    }
    sqlx::query(
        "INSERT INTO job_irreversible_actions(job_id, lease_epoch, action, node_id, error_code) \
         VALUES(?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(job_id, lease_epoch, action, node_id) DO NOTHING",
    )
    .bind(job)
    .bind(request.lease_epoch)
    .bind(request.action.as_str())
    .bind(node)
    .bind(request.error_code.filter(|_| failure_flow))
    .execute(&mut *tx)
    .await?;
    if !recorded(&mut tx, job, request.lease_epoch, request.action, node).await? {
        return Err(AppError::Internal);
    }
    tx.commit().await?;
    Ok(())
}

async fn recorded(
    conn: &mut SqliteConnection,
    job: Uuid,
    epoch: i64,
    action: IrreversibleAction,
    node: &str,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_irreversible_actions \
         WHERE job_id=?1 AND lease_epoch=?2 AND action=?3 AND node_id=?4)",
    )
    .bind(job)
    .bind(epoch)
    .bind(action.as_str())
    .bind(node)
    .fetch_one(conn)
    .await?)
}

/// Réserve le flux d'échec à ce bail : le central ne l'exécutera plus pour cet échec.
async fn take_failure_flow(conn: &mut SqliteConnection, job: Uuid, epoch: i64) -> AppResult<()> {
    if journal::publication_in_progress(conn, job, epoch)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict);
    }
    let taken = sqlx::query(
        "UPDATE jobs SET failure_flow_state=?2 WHERE id=?1 AND failure_flow_state IS NULL",
    )
    .bind(job)
    .bind(super::FailureFlowState::Done.as_str())
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let action = IrreversibleAction::FailureFlow;
    if taken == 1 || recorded(conn, job, epoch, action, FAILURE_FLOW_STEP_NODE).await? {
        Ok(())
    } else {
        Err(AppError::Conflict)
    }
}

/// Une fois le flux d'échec autorisé, ce bail ne publie plus rien et n'agit plus.
pub(super) async fn ensure_no_failure_flow(
    conn: &mut SqliteConnection,
    job: Uuid,
) -> AppResult<()> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND failure_flow_state IS NOT NULL)",
    )
    .bind(job)
    .fetch_one(conn)
    .await?;
    if taken {
        Err(AppError::Conflict)
    } else {
        Ok(())
    }
}

/// Première action qui exclut la reprise, toutes époques confondues : un relais ne fait pas oublier
/// l'appel du bail précédent. Le flux d'échec, qui porte le code de l'échec, passe d'abord.
pub(super) async fn blocking_action(
    conn: &mut SqliteConnection,
    job: Uuid,
) -> AppResult<Option<BlockingAction>> {
    let rows: Vec<(String, String, Option<JobErrorCode>)> = sqlx::query_as(
        "SELECT action, node_id, error_code FROM job_irreversible_actions WHERE job_id=?1 \
         ORDER BY error_code IS NULL, lease_epoch DESC, started_at DESC",
    )
    .bind(job)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().find_map(|(action, node_id, error_code)| {
        let action = IrreversibleAction::parse(&action)?;
        action.blocks_requeue().then_some(BlockingAction {
            action,
            node_id,
            error_code,
        })
    }))
}

/// Relance manuelle : l'administrateur assume la répétition.
pub(super) async fn clear(conn: &mut SqliteConnection, job: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM job_irreversible_actions WHERE job_id=?1")
        .bind(job)
        .execute(conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: IrreversibleAction, node_id: &str) -> AuthorizeAction {
        AuthorizeAction {
            worker_id: Uuid::nil(),
            lease_token: Uuid::nil(),
            lease_epoch: 1,
            action,
            node_id: node_id.into(),
            error_code: None,
        }
    }

    #[test]
    fn node_identifiers_are_bounded_and_the_failure_flow_uses_its_own() -> AppResult<()> {
        let call = |node: &str| request(IrreversibleAction::HttpCall, node);
        let longest = "é".repeat(ACTION_NODE_ID_MAX_CHARS);
        assert_eq!(recorded_node(&call(&longest))?, longest);
        let too_long = "x".repeat(ACTION_NODE_ID_MAX_CHARS + 1);
        for bad in ["", too_long.as_str()] {
            assert!(matches!(recorded_node(&call(bad)), Err(AppError::Invalid)));
        }
        let flow = request(IrreversibleAction::FailureFlow, "");
        assert_eq!(recorded_node(&flow)?, FAILURE_FLOW_STEP_NODE);
        Ok(())
    }
}
