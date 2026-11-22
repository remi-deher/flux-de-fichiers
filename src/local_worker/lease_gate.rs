//! Accord du central demandé par le worker intégré : la base est dans le même processus (ADR 0027).

use crate::{
    api::{self, AppState},
    domain::{AuthorizeAction, LeaseRequest},
    error::AppError,
    job_runner::action_gate::{
        ActionGate, ActionRefusal, ActionRequest, GateFuture, StopMemory, TaskStop,
    },
    worker::timing::{CALL_RETRY_DELAY, MAX_BROKEN_REPLIES},
};
use uuid::Uuid;

pub(super) struct LeaseGate {
    state: AppState,
    job: Uuid,
    lease: LeaseRequest,
    stop: StopMemory,
}

impl LeaseGate {
    pub(super) fn new(state: AppState, job: Uuid, lease: LeaseRequest) -> Self {
        Self {
            state,
            job,
            lease,
            stop: StopMemory::default(),
        }
    }

    fn halted(&self, stop: TaskStop) -> ActionRefusal {
        self.stop.record(stop);
        ActionRefusal::Stop(stop)
    }

    async fn authorize_action(&self, request: ActionRequest<'_>) -> Result<(), ActionRefusal> {
        let body = AuthorizeAction {
            worker_id: self.lease.worker_id,
            lease_token: self.lease.lease_token,
            lease_epoch: self.lease.lease_epoch,
            action: request.action,
            node_id: request.node_id.to_owned(),
            error_code: request.error_code,
        };
        let mut failures = 0;
        loop {
            match api::leases::authorize(&self.state, self.job, &body).await {
                Ok(()) => return Ok(()),
                Err(AppError::LeaseLost) => return Err(self.halted(TaskStop::LeaseLost)),
                Err(error @ (AppError::Conflict | AppError::Invalid)) => {
                    let reason = format!("action irréversible refusée par le central ({error})");
                    return Err(ActionRefusal::Excluded(reason));
                }
                Err(error) => {
                    tracing::warn!(job_id=%self.job, %error, "accord local sans réponse");
                    failures += 1;
                    if failures >= MAX_BROKEN_REPLIES {
                        return Err(self.halted(TaskStop::Unreachable));
                    }
                }
            }
            tokio::time::sleep(CALL_RETRY_DELAY).await;
        }
    }
}

impl ActionGate for LeaseGate {
    fn authorize<'a>(&'a self, request: ActionRequest<'a>) -> GateFuture<'a> {
        Box::pin(self.authorize_action(request))
    }

    fn halt(&self) -> Option<TaskStop> {
        self.stop.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::transfers::tests::setup, domain::IrreversibleAction, repository};

    #[tokio::test]
    async fn a_held_lease_is_granted_and_an_excluded_action_does_not_stop_it() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = setup(dir.path()).await?;
        let gate = LeaseGate::new(state, job.id, lease);
        let call = ActionRequest::block(IrreversibleAction::HttpCall, "appel");
        assert_eq!(gate.authorize(call).await, Ok(()));
        // Sans identifiant, l'action ne pourrait pas être inscrite.
        let unnamed = ActionRequest::block(IrreversibleAction::InPlace, "");
        let refused = gate.authorize(unnamed).await;
        assert!(matches!(refused, Err(ActionRefusal::Excluded(_))));
        assert_eq!(gate.halt(), None);
        Ok(())
    }

    #[tokio::test]
    async fn a_lost_lease_is_remembered() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = setup(dir.path()).await?;
        repository::cancel_job(&state.pool, job.id).await?;
        let gate = LeaseGate::new(state, job.id, lease);
        let call = ActionRequest::block(IrreversibleAction::HttpCall, "appel");
        let lost = TaskStop::LeaseLost;
        assert_eq!(gate.authorize(call).await, Err(ActionRefusal::Stop(lost)));
        assert_eq!(gate.halt(), Some(lost));
        Ok(())
    }
}
