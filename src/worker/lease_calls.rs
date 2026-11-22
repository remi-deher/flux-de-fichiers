//! Appels d'un titulaire de bail au central (ADR 0027) : battement tolérant, appels répétés tant que
//! le central est injoignable, accord, restitution, échec.

use super::{
    WorkerLoop,
    contact::{self, CentralReply, LeaseContact},
    timing::{
        CALL_RETRY_DELAY, CENTRAL_REQUEST_TIMEOUT, HEARTBEAT_INTERVAL, MAX_BROKEN_REPLIES,
        PATIENCE_MARGIN_SECONDS, RELEASE_TIMEOUT,
    },
};
use crate::{
    domain::{
        AuthorizeAction, Claim, Failure, JobErrorCode, JobStepEntry, LEASE_SECONDS, LeaseRequest,
        LeaseTerms, StepsReport,
    },
    job_runner::{
        JobRunError,
        action_gate::{ActionGate, ActionRefusal, ActionRequest, GateFuture, TaskStop},
    },
};
use forgearr_shared::jobs::UNREACHABLE_AFTER_SECONDS;
use reqwest::{Method, RequestBuilder, StatusCode};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Refus qu'un battement doit confirmer : ils peuvent venir d'un bail perdu.
const DOUBTFUL_CODES: [&str; 2] = ["lease_expired_or_conflict", "not_found"];

fn seconds(value: i64) -> Duration {
    Duration::from_secs(u64::try_from(value).unwrap_or_default())
}

/// Réponse (ou absence de réponse) à une requête.
pub(super) struct Attempt {
    pub(super) reply: CentralReply,
    status: Option<StatusCode>,
    code: Option<String>,
    unreachable_timeout: Option<i64>,
    body: Vec<u8>,
}

impl Attempt {
    fn silent() -> Self {
        Self {
            reply: CentralReply::Unavailable,
            status: None,
            code: None,
            unreachable_timeout: None,
            body: Vec::new(),
        }
    }

    pub(super) fn json<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
        Ok(serde_json::from_slice(&self.body)?)
    }

    /// Refus qui peut venir d'un bail perdu (seule forme connue d'un ancien central).
    fn doubtful(&self) -> bool {
        let refused = self.reply == CentralReply::Refused;
        let status = matches!(
            self.status,
            Some(StatusCode::CONFLICT | StatusCode::NOT_FOUND)
        );
        refused
            && status
            && self
                .code
                .as_deref()
                .is_some_and(|c| DOUBTFUL_CODES.contains(&c))
    }
}

/// Bornes d'un appel.
#[derive(Clone, Copy, Debug)]
pub(super) struct CallBudget {
    pub(super) attempt: Duration,
    /// Durée totale des répétitions ; nulle : appel unique.
    pub(super) overall: Duration,
}

/// Appel resté sans succès.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum CallFailure {
    Stop(TaskStop),
    Refused(Option<String>),
    /// Le central répond aux battements mais pas à cet appel.
    Exhausted,
}

impl From<CallFailure> for anyhow::Error {
    fn from(failure: CallFailure) -> Self {
        match failure {
            CallFailure::Stop(stop) => stop.into(),
            CallFailure::Refused(code) => run_error(refusal_code(code.as_deref())),
            CallFailure::Exhausted => run_error(JobErrorCode::TransferFailed),
        }
    }
}

fn run_error(code: JobErrorCode) -> anyhow::Error {
    JobRunError::simple(code, code.as_str()).into()
}

/// Code d'échec de la tâche déduit d'un refus du central.
fn refusal_code(code: Option<&str>) -> JobErrorCode {
    if code == Some(JobErrorCode::SourceChanged.as_str()) {
        JobErrorCode::SourceChanged
    } else {
        JobErrorCode::TransferFailed
    }
}

/// Le bail d'une tâche et son contact avec le central.
#[derive(Clone)]
pub(super) struct TaskLease {
    pub(super) runner: WorkerLoop,
    pub(super) job_id: Uuid,
    worker_id: Uuid,
    token: Uuid,
    epoch: i64,
    /// Absentes d'un ancien central : ni accord, ni restitution, ni attente longue.
    terms: Option<LeaseTerms>,
    pub(super) contact: Arc<LeaseContact>,
}

impl TaskLease {
    pub(super) fn new(runner: &WorkerLoop, claim: &Claim, worker_id: Uuid) -> Self {
        Self {
            runner: runner.clone(),
            job_id: claim.job_id,
            worker_id,
            token: claim.lease_token,
            epoch: claim.lease_epoch,
            terms: claim.lease_terms,
            contact: Arc::default(),
        }
    }

    pub(super) fn supports_reachability(&self) -> bool {
        self.terms.is_some()
    }

    /// Accord à demander avant une action irréversible ; aucun face à un ancien central.
    pub(super) fn gate(&self) -> Option<&dyn ActionGate> {
        if self.supports_reachability() {
            Some(self)
        } else {
            None
        }
    }

    pub(super) fn request_body(&self) -> LeaseRequest {
        LeaseRequest {
            worker_id: self.worker_id,
            lease_token: self.token,
            lease_epoch: self.epoch,
            telemetry: None,
        }
    }

    /// Requête `/api/v1/jobs/{id}/{action}`.
    pub(super) fn request(&self, method: Method, action: &str) -> RequestBuilder {
        let path = format!("/api/v1/jobs/{}/{action}", self.job_id);
        self.runner.request(method, &path)
    }

    /// Même requête, bail porté par les en-têtes.
    pub(super) fn leased(&self, method: Method, action: &str) -> RequestBuilder {
        self.request(method, action)
            .header("x-worker-id", self.worker_id.to_string())
            .header("x-lease-token", self.token.to_string())
            .header("x-lease-epoch", self.epoch.to_string())
    }

    /// Temps pendant lequel un appel attend le retour du central : le délai du dernier battement, à
    /// défaut celui de la réservation.
    pub(super) fn patience(&self) -> Duration {
        match self.terms {
            Some(terms) => seconds(
                self.contact
                    .announced_timeout()
                    .unwrap_or(terms.unreachable_timeout_seconds)
                    .saturating_add(PATIENCE_MARGIN_SECONDS),
            ),
            None => seconds(LEASE_SECONDS),
        }
    }

    pub(super) fn budget(&self, attempt: Duration) -> CallBudget {
        CallBudget {
            attempt,
            overall: self.patience(),
        }
    }

    fn exhausted(&self) -> CallFailure {
        if self.contact.silence() < seconds(UNREACHABLE_AFTER_SECONDS) {
            CallFailure::Exhausted
        } else {
            CallFailure::Stop(TaskStop::Unreachable)
        }
    }

    /// Répète l'appel tant que le central est injoignable, dans les bornes de `budget`.
    pub(super) async fn call(
        &self,
        budget: CallBudget,
        build: impl Fn() -> RequestBuilder + Send + Sync,
    ) -> Result<Attempt, CallFailure> {
        let started = Instant::now();
        let mut broken = 0;
        let mut confirmed = false;
        loop {
            let attempt = self.runner.attempt(build().timeout(budget.attempt)).await;
            match attempt.reply {
                CentralReply::Accepted => {
                    self.contact.touch();
                    return Ok(attempt);
                }
                CentralReply::LeaseLost => return Err(CallFailure::Stop(TaskStop::LeaseLost)),
                CentralReply::Refused => {
                    self.contact.touch();
                    if attempt.doubtful() && !confirmed {
                        self.confirm_held().await?;
                        confirmed = true;
                        if !budget.overall.is_zero() {
                            continue;
                        }
                    }
                    return Err(CallFailure::Refused(attempt.code));
                }
                CentralReply::Broken => {
                    broken += 1;
                    if broken >= MAX_BROKEN_REPLIES {
                        return Err(CallFailure::Refused(attempt.code));
                    }
                }
                CentralReply::Unavailable => {}
            }
            if started.elapsed() >= budget.overall || self.contact.silence() >= self.patience() {
                return Err(self.exhausted());
            }
            tokio::time::sleep(CALL_RETRY_DELAY).await;
        }
    }

    /// Le bail est-il toujours tenu ? Attend une réponse du central, dans la limite de la patience.
    async fn confirm_held(&self) -> Result<(), CallFailure> {
        let started = Instant::now();
        loop {
            match self.beat(self.request_body()).await {
                CentralReply::Accepted => return Ok(()),
                CentralReply::LeaseLost => return Err(CallFailure::Stop(TaskStop::LeaseLost)),
                _ if started.elapsed() >= self.patience() => return Err(self.exhausted()),
                _ => tokio::time::sleep(CALL_RETRY_DELAY).await,
            }
        }
    }

    /// Un battement. Un ancien central n'a pas de code propre à la perte du bail.
    async fn beat(&self, body: LeaseRequest) -> CentralReply {
        let request = self
            .request(Method::POST, "heartbeat")
            .timeout(CENTRAL_REQUEST_TIMEOUT)
            .json(&body);
        let attempt = self.runner.attempt(request).await;
        match attempt.reply {
            CentralReply::Accepted => {
                self.contact.touch();
                self.contact.announce_timeout(attempt.unreachable_timeout);
                CentralReply::Accepted
            }
            CentralReply::Refused if !self.supports_reachability() && attempt.doubtful() => {
                CentralReply::LeaseLost
            }
            other => other,
        }
    }

    /// Bat tant que le bail est tenu : une coupure n'arrête rien, seul un refus définitif rend la main.
    pub(super) async fn heartbeat_until_lost(&self) {
        let probe =
            crate::telemetry::TelemetryProbe::with_storage_path(&self.runner.config.work_dir);
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            let mut body = self.request_body();
            body.telemetry = Some(probe.sample());
            match self.beat(body).await {
                CentralReply::Accepted => {}
                CentralReply::LeaseLost => return,
                _ => {
                    tracing::warn!(job_id=%self.job_id, "battement sans réponse : le travail continue");
                }
            }
        }
    }

    pub(super) async fn report_steps(
        &self,
        steps: &[JobStepEntry],
        media_info: Option<&serde_json::Value>,
    ) -> Result<(), CallFailure> {
        if steps.is_empty() {
            return Ok(());
        }
        let report = StepsReport {
            worker_id: self.worker_id,
            lease_token: self.token,
            lease_epoch: self.epoch,
            steps: steps.to_vec(),
            media_info: media_info.cloned(),
        };
        self.call(self.budget(CENTRAL_REQUEST_TIMEOUT), || {
            self.request(Method::POST, "steps").json(&report)
        })
        .await?;
        Ok(())
    }

    /// Déclare l'échec du traitement, après ses étapes.
    pub(super) async fn report_failure(
        &self,
        code: JobErrorCode,
        steps: &[JobStepEntry],
    ) -> Result<(), CallFailure> {
        match self.report_steps(steps, None).await {
            Err(CallFailure::Stop(stop)) => return Err(CallFailure::Stop(stop)),
            Err(_) => tracing::warn!(job_id=%self.job_id, "step report rejected or unavailable"),
            Ok(()) => {}
        }
        let failure = Failure {
            worker_id: self.worker_id,
            lease_token: self.token,
            lease_epoch: self.epoch,
            error_code: code.as_str().into(),
        };
        self.call(self.budget(CENTRAL_REQUEST_TIMEOUT), || {
            self.request(Method::POST, "failure").json(&failure)
        })
        .await?;
        Ok(())
    }

    /// Rend la tâche (arrêt propre). Sans réponse, le redémarrage ou l'échéance tranche.
    pub(super) async fn release(&self) {
        if !self.supports_reachability() {
            return;
        }
        let request = self
            .request(Method::POST, "release")
            .timeout(RELEASE_TIMEOUT)
            .json(&self.request_body());
        let reply = self.runner.attempt(request).await.reply;
        tracing::info!(job_id=%self.job_id, ?reply, "restitution de la tâche");
    }

    async fn authorize_action(&self, request: ActionRequest<'_>) -> Result<(), ActionRefusal> {
        let body = AuthorizeAction {
            worker_id: self.worker_id,
            lease_token: self.token,
            lease_epoch: self.epoch,
            action: request.action,
            node_id: request.node_id.to_owned(),
            error_code: request.error_code,
        };
        let granted = self
            .call(self.budget(CENTRAL_REQUEST_TIMEOUT), || {
                self.request(Method::POST, "irreversible/authorize")
                    .json(&body)
            })
            .await;
        match granted {
            Ok(_) => Ok(()),
            Err(CallFailure::Stop(stop)) => {
                self.contact.stop.record(stop);
                Err(ActionRefusal::Stop(stop))
            }
            Err(CallFailure::Refused(code)) => Err(ActionRefusal::Excluded(format!(
                "action irréversible refusée par le central ({})",
                code.unwrap_or_default()
            ))),
            Err(CallFailure::Exhausted) => Err(ActionRefusal::Excluded(
                "action irréversible sans accord du central".to_owned(),
            )),
        }
    }
}

impl ActionGate for TaskLease {
    fn authorize<'a>(&'a self, request: ActionRequest<'a>) -> GateFuture<'a> {
        Box::pin(self.authorize_action(request))
    }

    fn halt(&self) -> Option<TaskStop> {
        self.contact.stop.get()
    }
}

impl WorkerLoop {
    /// Envoie la requête et lit la réponse en entier ; une coupure vaut un central injoignable.
    pub(super) async fn attempt(&self, request: RequestBuilder) -> Attempt {
        let Ok(response) = request.send().await else {
            return Attempt::silent();
        };
        let status = response.status();
        let unreachable_timeout = contact::announced_timeout(response.headers());
        let Ok(body) = response.bytes().await else {
            return Attempt::silent();
        };
        let code = contact::error_code(&body);
        Attempt {
            reply: contact::classify(status, code.as_deref()),
            status: Some(status),
            code,
            unreachable_timeout,
            body: body.to_vec(),
        }
    }

    /// Répète une requête hors bail tant que le central est injoignable, au plus `limit`.
    pub(super) async fn attempt_for(
        &self,
        limit: Duration,
        build: impl Fn() -> RequestBuilder + Send + Sync,
    ) -> Attempt {
        let started = Instant::now();
        loop {
            let attempt = self.attempt(build()).await;
            let waiting = matches!(
                attempt.reply,
                CentralReply::Unavailable | CentralReply::Broken
            );
            if !waiting || started.elapsed() >= limit {
                return attempt;
            }
            tokio::time::sleep(CALL_RETRY_DELAY).await;
        }
    }
}

#[cfg(test)]
#[path = "lease_calls_tests.rs"]
mod tests;
