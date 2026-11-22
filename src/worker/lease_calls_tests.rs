//! Appels d'un titulaire face à un central factice (ADR 0027).

use super::*;
use crate::{
    worker::{WorkerConfig, held_tasks::HeldTasks},
    worker_http::WorkerState,
};
use axum::{Router, body::Body, extract::State, http::Uri, response::Response};
use forgearr_shared::jobs::{IrreversibleAction, UNREACHABLE_TIMEOUT_HEADER};
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

const LOST: (u16, &str) = (409, r#"{"error":{"code":"lease_lost"}}"#);
const CONFLICT: (u16, &str) = (409, r#"{"error":{"code":"lease_expired_or_conflict"}}"#);
const INTERNAL: (u16, &str) = (500, r#"{"error":{"code":"internal_error"}}"#);
const PROXY: (u16, &str) = (502, "<html>");
const ACCEPTED: (u16, &str) = (204, "");
/// Borne d'un test qui doit se terminer de lui-même.
const TEST_LIMIT: Duration = Duration::from_secs(10);

type Script = HashMap<&'static str, Vec<(u16, &'static str)>>;

/// Central factice : réponses dans l'ordre par dernière partie du chemin ; la dernière se répète.
#[derive(Clone, Default)]
struct FakeCentral {
    script: Arc<Mutex<Script>>,
    calls: Arc<Mutex<Vec<String>>>,
    /// Délai d'injoignabilité rendu avec chaque réponse.
    timeout: Option<i64>,
}

impl FakeCentral {
    fn answers(self, action: &'static str, replies: &[(u16, &'static str)]) -> Self {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(action, replies.to_vec());
        self
    }

    fn announces(mut self, timeout: i64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn count(&self, action: &str) -> usize {
        let calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        calls.iter().filter(|call| *call == action).count()
    }

    fn reply(&self, action: &str) -> (u16, &'static str) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(action.to_owned());
        let mut script = self.script.lock().unwrap_or_else(PoisonError::into_inner);
        match script.get_mut(action) {
            Some(replies) if replies.len() > 1 => replies.remove(0),
            Some(replies) => replies.first().copied().unwrap_or(ACCEPTED),
            None => ACCEPTED,
        }
    }
}

async fn handle(State(central): State<FakeCentral>, uri: Uri) -> Response {
    let action = uri.path().rsplit('/').next().unwrap_or_default();
    let (status, body) = central.reply(action);
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = StatusCode::from_u16(status).unwrap_or_default();
    if let Some(timeout) = central.timeout {
        let headers = response.headers_mut();
        headers.insert(UNREACHABLE_TIMEOUT_HEADER, timeout.into());
    }
    response
}

async fn lease(central: &FakeCentral, terms: Option<LeaseTerms>) -> anyhow::Result<TaskLease> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/", listener.local_addr()?).parse()?;
    let app = Router::new().fallback(handle).with_state(central.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let (_, shutdown) = tokio::sync::watch::channel(false);
    let runner = WorkerLoop {
        client: reqwest::Client::new(),
        config: WorkerConfig {
            url,
            key: "cle".into(),
            name: "worker-test".into(),
            work_dir: "var/work".into(),
            tools_cache_dir: "var/tools".into(),
            ca: None,
            max_bytes: 1024,
            transfer_url: None,
            transfer_port: None,
        },
        status: WorkerState::new("worker-test", "http://central.invalid"),
        boot_id: Uuid::new_v4(),
        held: HeldTasks::default(),
        shutdown,
    };
    Ok(TaskLease {
        runner,
        job_id: Uuid::new_v4(),
        worker_id: Uuid::new_v4(),
        token: Uuid::new_v4(),
        epoch: 1,
        terms,
        contact: Arc::default(),
    })
}

fn terms() -> Option<LeaseTerms> {
    Some(LeaseTerms {
        unreachable_timeout_seconds: 600,
    })
}

fn call_request() -> ActionRequest<'static> {
    ActionRequest::block(IrreversibleAction::HttpCall, "appel")
}

#[tokio::test]
async fn a_call_waits_for_an_unreachable_central() -> anyhow::Result<()> {
    let central = FakeCentral::default().answers("authorize", &[PROXY, PROXY, ACCEPTED]);
    let lease = lease(&central, terms()).await?;
    assert_eq!(lease.authorize(call_request()).await, Ok(()));
    assert_eq!(central.count("authorize"), 3);
    assert_eq!(lease.halt(), None);
    Ok(())
}

#[tokio::test]
async fn a_lost_lease_stops_the_task_at_once() -> anyhow::Result<()> {
    let central = FakeCentral::default().answers("authorize", &[LOST]);
    let lease = lease(&central, terms()).await?;
    let refused = lease.authorize(call_request()).await;
    assert_eq!(refused, Err(ActionRefusal::Stop(TaskStop::LeaseLost)));
    assert_eq!(lease.halt(), Some(TaskStop::LeaseLost));
    assert_eq!(central.count("authorize"), 1);
    Ok(())
}

#[tokio::test]
async fn an_ambiguous_refusal_is_confirmed_by_a_heartbeat() -> anyhow::Result<()> {
    let held = FakeCentral::default().answers("authorize", &[CONFLICT]);
    let lease_held = lease(&held, terms()).await?;
    let refused = lease_held.authorize(call_request()).await;
    assert!(matches!(refused, Err(ActionRefusal::Excluded(_))));
    assert_eq!(lease_held.halt(), None);
    assert_eq!((held.count("heartbeat"), held.count("authorize")), (1, 2));

    let lost = FakeCentral::default()
        .answers("authorize", &[CONFLICT])
        .answers("heartbeat", &[LOST]);
    let lease_lost = lease(&lost, terms()).await?;
    let refused = lease_lost.authorize(call_request()).await;
    assert_eq!(refused, Err(ActionRefusal::Stop(TaskStop::LeaseLost)));
    Ok(())
}

#[tokio::test]
async fn a_repeated_internal_error_is_a_declared_failure() -> anyhow::Result<()> {
    let central = FakeCentral::default().answers("steps", &[INTERNAL]);
    let lease = lease(&central, terms()).await?;
    let budget = lease.budget(CENTRAL_REQUEST_TIMEOUT);
    let failed = lease
        .call(budget, || lease.request(Method::POST, "steps"))
        .await;
    let code = Some("internal_error".to_owned());
    assert!(matches!(failed, Err(CallFailure::Refused(reason)) if reason == code));
    Ok(())
}

#[tokio::test]
async fn an_exhausted_wait_is_declared_only_if_the_central_answers() -> anyhow::Result<()> {
    let central = FakeCentral::default().answers("ready", &[PROXY]);
    let lease = lease(&central, terms()).await?;
    let once = CallBudget {
        attempt: CENTRAL_REQUEST_TIMEOUT,
        overall: Duration::ZERO,
    };
    let failed = lease
        .call(once, || lease.request(Method::POST, "ready"))
        .await;
    assert!(matches!(failed, Err(CallFailure::Exhausted)));
    Ok(())
}

#[tokio::test]
async fn heartbeats_survive_a_cut_and_end_on_a_definitive_refusal() -> anyhow::Result<()> {
    let central = FakeCentral::default().answers("heartbeat", &[PROXY, INTERNAL, CONFLICT, LOST]);
    let current = lease(&central, terms()).await?;
    for _ in 0..3 {
        let reply = current.beat(current.request_body()).await;
        assert!(!matches!(
            reply,
            CentralReply::Accepted | CentralReply::LeaseLost
        ));
    }
    let reply = current.beat(current.request_body()).await;
    assert_eq!(reply, CentralReply::LeaseLost);

    // Un ancien central n'a que ce conflit pour dire la perte du bail.
    let old_central = FakeCentral::default().answers("heartbeat", &[CONFLICT]);
    let old = lease(&old_central, None).await?;
    let reply = old.beat(old.request_body()).await;
    assert_eq!(reply, CentralReply::LeaseLost);
    Ok(())
}

#[tokio::test]
async fn the_work_goes_on_through_a_cut_and_stops_only_when_the_lease_is_lost() -> anyhow::Result<()>
{
    let replies = [PROXY, INTERNAL, ACCEPTED, PROXY, CONFLICT, LOST];
    let central = FakeCentral::default().answers("heartbeat", &replies);
    let lease = lease(&central, terms()).await?;
    tokio::time::timeout(TEST_LIMIT, lease.heartbeat_until_lost()).await?;
    assert_eq!(central.count("heartbeat"), replies.len());
    Ok(())
}

#[tokio::test]
async fn the_patience_follows_the_timeout_of_the_last_heartbeat() -> anyhow::Result<()> {
    const CHANGED: i64 = 3600;
    let central = FakeCentral::default().announces(CHANGED);
    let current = lease(&central, terms()).await?;
    assert_eq!(current.patience(), seconds(600 + PATIENCE_MARGIN_SECONDS));
    current.beat(current.request_body()).await;
    assert_eq!(
        current.patience(),
        seconds(CHANGED + PATIENCE_MARGIN_SECONDS)
    );
    // Bail donné par un ancien central, mis à jour depuis : il garde son échéance courte.
    let old = lease(&central, None).await?;
    old.beat(old.request_body()).await;
    assert_eq!(old.patience(), seconds(LEASE_SECONDS));
    Ok(())
}

#[tokio::test]
async fn a_call_outside_a_lease_waits_within_its_limit() -> anyhow::Result<()> {
    let central = FakeCentral::default().answers("report", &[PROXY, INTERNAL, ACCEPTED]);
    let recovering = lease(&central, terms()).await?;
    let build = || recovering.request(Method::POST, "report");
    let attempt = recovering.runner.attempt_for(TEST_LIMIT, build).await;
    assert_eq!(attempt.reply, CentralReply::Accepted);
    let down = FakeCentral::default().answers("report", &[PROXY]);
    let cut = lease(&down, terms()).await?;
    let build = || cut.request(Method::POST, "report");
    let attempt = cut.runner.attempt_for(Duration::ZERO, build).await;
    assert_eq!(attempt.reply, CentralReply::Unavailable);
    assert_eq!(down.count("report"), 1);
    Ok(())
}

#[tokio::test]
async fn an_old_central_is_asked_for_nothing_new() -> anyhow::Result<()> {
    let central = FakeCentral::default();
    let old = lease(&central, None).await?;
    assert!(old.gate().is_none());
    old.release().await;
    assert_eq!(central.count("release"), 0);
    assert_eq!(old.patience(), seconds(LEASE_SECONDS));

    let current = lease(&central, terms()).await?;
    assert!(current.gate().is_some());
    current.release().await;
    assert_eq!(central.count("release"), 1);
    assert_eq!(current.patience(), seconds(600 + PATIENCE_MARGIN_SECONDS));
    Ok(())
}

#[test]
fn a_refused_publication_tells_source_changes_apart() {
    let changed = JobErrorCode::SourceChanged;
    assert_eq!(refusal_code(Some(changed.as_str())), changed);
    for other in [Some("lease_expired_or_conflict"), None] {
        assert_eq!(refusal_code(other), JobErrorCode::TransferFailed);
    }
}
