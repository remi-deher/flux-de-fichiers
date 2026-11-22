//! Compatibilité de l'ADR 0027 avec les workers 0.79.0, et routes de bail.

mod common;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, request},
};
use common::leases::{declaration, expire, pool, queued_job, silent_for};
use forgearr::{
    api::{AppState, router},
    api_keys::install_known_key,
    config::Config,
    domain::{
        Claim, ClaimRequest, JobErrorCode, JobStatus, MAX_ATTEMPTS, Register, UpdateFlowRequest,
    },
    repository::{self, MAX_FREE_RELEASES},
};
use forgearr_shared::{
    Role,
    jobs::{LeaseProtocol, UNREACHABLE_AFTER_SECONDS, UNREACHABLE_TIMEOUT_HEADER},
    relay::{RelayRequest, RelayTarget},
};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

const WORKER_NAME: &str = "test-worker";
const OTHER_KEY: &str = "fdf_c0000001_0011223344556677001122334455667700112233445566aa";
const BODY_LIMIT: usize = 1 << 20;

struct Harness {
    app: Router,
    pool: SqlitePool,
    _dir: tempfile::TempDir,
}

async fn harness() -> anyhow::Result<Harness> {
    let dir = tempfile::tempdir()?;
    let pool = pool().await?;
    common::install_keys(&pool).await?;
    install_known_key(&pool, OTHER_KEY, "autre-worker", Role::Worker).await?;
    let config = Config {
        bind: ([127, 0, 0, 1], 3000).into(),
        database_url: "sqlite::memory:".into(),
        data_dir: dir.path().to_path_buf(),
        input_dir: dir.path().join("input"),
        config_dir: dir.path().join("config"),
        tools_cache_dir: dir.path().join("tools"),
        plugins_dir: None,
        static_dir: dir.path().join("static"),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    std::fs::create_dir_all(&config.config_dir)?;
    let app = router(AppState::new(pool.clone(), Arc::new(config)));
    Ok(Harness {
        app,
        pool,
        _dir: dir,
    })
}

impl Harness {
    async fn call(
        &self,
        method: &str,
        uri: &str,
        key: &str,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        self.send(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {key}")),
            body,
        )
        .await
    }

    /// Même appel, bail porté par les en-têtes.
    async fn leased(
        &self,
        method: &str,
        uri: &str,
        lease: &Value,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let text = |field: &str| lease[field].as_str().unwrap_or_default().to_owned();
        self.send(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {}", common::WORKER_KEY))
                .header("x-worker-id", text("worker_id"))
                .header("x-lease-token", text("lease_token"))
                .header("x-lease-epoch", lease["lease_epoch"].to_string()),
            body,
        )
        .await
    }

    async fn send(
        &self,
        request: request::Builder,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let request = request
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body)?))?;
        let response = self.app.clone().oneshot(request).await?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), BODY_LIMIT).await?;
        Ok((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }

    async fn register(&self, boot: Option<Uuid>, key: &str) -> anyhow::Result<Uuid> {
        let body = json!({"name": WORKER_NAME, "boot_id": boot});
        let (status, worker) = self.call("POST", "/api/v1/workers", key, body).await?;
        anyhow::ensure!(status == StatusCode::OK, "{status}");
        Ok(serde_json::from_value(worker["id"].clone())?)
    }

    /// Réserve par l'API ; rend la tâche et le corps de bail à renvoyer.
    async fn claim(&self, worker: Uuid, protocol: Option<i64>) -> anyhow::Result<(Claim, Value)> {
        let body = json!({"worker_id": worker, "lease_protocol": protocol});
        let (status, claim) = self
            .call("POST", "/api/v1/claims", common::WORKER_KEY, body)
            .await?;
        anyhow::ensure!(status == StatusCode::OK, "{status}");
        let claim: Claim = serde_json::from_value(claim)?;
        let lease = json!({"worker_id": worker, "lease_token": claim.lease_token,
                           "lease_epoch": claim.lease_epoch});
        Ok((claim, lease))
    }
}

fn with_action(lease: &Value, action: &str, node: &str) -> Value {
    let mut body = lease.clone();
    body["action"] = json!(action);
    body["node_id"] = json!(node);
    body
}

#[test]
fn old_and_new_messages_understand_each_other() -> Result<(), serde_json::Error> {
    let register: Register = serde_json::from_value(json!({"name": "w"}))?;
    assert!(register.boot_id.is_none());
    assert!(serde_json::to_value(&register)?.get("boot_id").is_none());
    let request: ClaimRequest = serde_json::from_value(json!({"worker_id": Uuid::nil()}))?;
    assert!(request.lease_protocol.is_none() && request.active_jobs.is_none());
    let encoded = serde_json::to_value(&request)?;
    assert!(encoded.get("lease_protocol").is_none());
    let claim: Claim = serde_json::from_value(json!({
        "job_id": Uuid::nil(), "lease_token": Uuid::nil(), "source_name": "a.mkv",
        "source_sha256": "ab", "source_bytes": 1
    }))?;
    assert!(claim.lease_terms.is_none());
    assert!(serde_json::to_value(&claim)?.get("lease_terms").is_none());
    Ok(())
}

#[tokio::test]
async fn an_old_worker_never_gets_an_automatic_requeue() -> anyhow::Result<()> {
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let (claim, lease) = h.claim(worker, None).await?;
    assert_eq!(claim.job_id, job);
    let left: i64 = sqlx::query_scalar(
        "SELECT unixepoch(leased_until) - unixepoch('now') FROM jobs WHERE id=?1",
    )
    .bind(job)
    .fetch_one(&h.pool)
    .await?;
    assert!(
        left <= forgearr::domain::LEASE_SECONDS,
        "bail court conservé"
    );
    let beat = format!("/api/v1/jobs/{job}/heartbeat");
    let (status, _) = h
        .call("POST", &beat, common::WORKER_KEY, lease.clone())
        .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    expire(&h.pool, job).await?;
    let failed = repository::get_job(&h.pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    let (status, body) = h.call("POST", &beat, common::WORKER_KEY, lease).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "lease_lost");
    Ok(())
}

#[tokio::test]
async fn lease_routes_require_the_identity_of_the_worker() -> anyhow::Result<()> {
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let protocol = Some(LeaseProtocol::LATEST.number());
    let (claim, lease) = h.claim(worker, protocol).await?;
    assert!(claim.lease_terms.is_some());
    let authorize = format!("/api/v1/jobs/{job}/irreversible/authorize");
    let release = format!("/api/v1/jobs/{job}/release");
    let call = with_action(&lease, "http_call", "n1");
    for (uri, body) in [(&authorize, &call), (&release, &lease)] {
        let (status, _) = h.call("POST", uri, OTHER_KEY, body.clone()).await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let in_place = with_action(&lease, "in_place", "n1");
    let (status, _) = h
        .call("POST", &authorize, common::WORKER_KEY, in_place)
        .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let infos = format!("/api/v1/jobs/leases?ids={job}");
    let (status, body) = h
        .call("GET", &infos, common::ADMIN_KEY, Value::Null)
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["reachability"], "reachable");

    let (status, _) = h
        .call("POST", &release, common::WORKER_KEY, lease.clone())
        .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        repository::get_job(&h.pool, job).await?.status,
        JobStatus::Queued
    );
    let (status, body) = h.call("POST", &authorize, common::WORKER_KEY, call).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "lease_lost");
    Ok(())
}

#[tokio::test]
async fn only_the_key_of_the_worker_announces_its_boot() -> anyhow::Result<()> {
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let boot = Some(Uuid::new_v4());
    let worker = h.register(boot, common::WORKER_KEY).await?;
    h.claim(worker, Some(LeaseProtocol::LATEST.number()))
        .await?;
    h.register(Some(Uuid::new_v4()), OTHER_KEY).await?;
    h.register(boot, common::WORKER_KEY).await?;
    h.register(None, common::WORKER_KEY).await?;
    assert_eq!(
        repository::get_job(&h.pool, job).await?.status,
        JobStatus::Running
    );
    h.register(Some(Uuid::new_v4()), common::WORKER_KEY).await?;
    assert_eq!(
        repository::get_job(&h.pool, job).await?.status,
        JobStatus::Queued
    );
    Ok(())
}

#[tokio::test]
async fn a_lost_claim_answer_is_recovered_by_the_list_of_held_jobs() -> anyhow::Result<()> {
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let protocol = LeaseProtocol::LATEST.number();
    let (lost, _) = h.claim(worker, Some(protocol)).await?;
    silent_for(&h.pool, job, UNREACHABLE_AFTER_SECONDS + 1).await?;
    let body = json!({"worker_id": worker, "lease_protocol": protocol, "active_jobs": []});
    let (status, claim) = h
        .call("POST", "/api/v1/claims", common::WORKER_KEY, body)
        .await?;
    assert_eq!(status, StatusCode::OK);
    let claim: Claim = serde_json::from_value(claim)?;
    assert_eq!(claim.job_id, job);
    assert!(claim.lease_epoch > lost.lease_epoch);
    Ok(())
}

/// `lease` complété des champs de `extra`.
fn with(lease: &Value, extra: Value) -> Value {
    let mut body = lease.clone();
    if let (Some(body), Value::Object(extra)) = (body.as_object_mut(), extra) {
        body.extend(extra);
    }
    body
}

#[tokio::test]
async fn every_lease_route_refuses_the_former_lease() -> anyhow::Result<()> {
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let protocol = Some(LeaseProtocol::LATEST.number());
    let (_, old) = h.claim(worker, protocol).await?;
    expire(&h.pool, job).await?;
    let (_, fresh) = h.claim(worker, protocol).await?;
    let relay = json!({"request": {
        "target": {"mode": "worker", "worker_id": worker}, "resumeNodeId": "suite"
    }});
    let mut answers = Vec::new();
    for (action, body) in [
        ("heartbeat", old.clone()),
        ("progress", with(&old, json!({"progress": 1}))),
        ("steps", with(&old, json!({"steps": []}))),
        (
            "failure",
            with(&old, json!({"error_code": JobErrorCode::OperationFailed})),
        ),
        ("relay", with(&old, relay)),
        ("release", old.clone()),
        (
            "irreversible/authorize",
            with_action(&old, "in_place", "n1"),
        ),
    ] {
        let uri = format!("/api/v1/jobs/{job}/{action}");
        let answer = h.call("POST", &uri, common::WORKER_KEY, body).await?;
        answers.push((action, answer));
    }
    let artifact = serde_json::to_value(declaration())?;
    for (method, action, body) in [
        ("POST", "source/ticket", Value::Null),
        ("PUT", "artifacts/working", artifact.clone()),
        ("PUT", "artifacts/result", artifact),
        ("POST", "artifacts/working/received", Value::Null),
        ("POST", "result/ready", json!({"extension": "mkv"})),
        ("POST", "result/authorize", Value::Null),
        ("POST", "result/applied", json!("published")),
        ("POST", "result/applied", json!("failed")),
    ] {
        let uri = format!("/api/v1/jobs/{job}/{action}");
        answers.push((action, h.leased(method, &uri, &old, body).await?));
    }
    for (action, (status, body)) in answers {
        assert_eq!(status, StatusCode::CONFLICT, "{action}");
        assert_eq!(body["error"]["code"], "lease_lost", "{action}");
    }
    let beat = format!("/api/v1/jobs/{job}/heartbeat");
    let (status, _) = h.call("POST", &beat, common::WORKER_KEY, fresh).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    Ok(())
}

#[tokio::test]
async fn a_heartbeat_gives_the_current_timeout() -> anyhow::Result<()> {
    const CHANGED: i64 = 3600;
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let (_, lease) = h
        .claim(worker, Some(LeaseProtocol::LATEST.number()))
        .await?;
    repository::set_setting(
        &h.pool,
        forgearr::runtime_settings::RUNTIME_SETTINGS_KEY,
        &json!({"unreachable_timeout_seconds": CHANGED}).to_string(),
    )
    .await?;
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/jobs/{job}/heartbeat"))
        .header("authorization", format!("Bearer {}", common::WORKER_KEY))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&lease)?))?;
    let response = h.app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let announced = response.headers().get(UNREACHABLE_TIMEOUT_HEADER);
    assert_eq!(
        announced.and_then(|value| value.to_str().ok()),
        Some(CHANGED.to_string().as_str())
    );
    Ok(())
}

#[tokio::test]
async fn an_unreadable_secret_gives_the_lease_back_instead_of_failing_the_job() -> anyhow::Result<()>
{
    let h = harness().await?;
    let definition = json!({
        "nodes": [{"id": "n1", "type": "webhook", "name": "appel",
                   "config": {"custom_headers": {"Authorization": "enc:v2:inconnue:00"}}}],
        "connections": []
    });
    let flow = UpdateFlowRequest {
        name: "secret".into(),
        description: None,
        file_mask: "*".into(),
        is_default: true,
        definition: definition.to_string(),
    };
    repository::save_flow(&h.pool, "flow.secret", &flow).await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let body = json!({"worker_id": worker, "lease_protocol": LeaseProtocol::LATEST.number()});
    let claims = i64::from(MAX_ATTEMPTS) + MAX_FREE_RELEASES;
    for _ in 0..claims {
        let waiting = repository::get_job(&h.pool, job).await?;
        assert_eq!(waiting.status, JobStatus::Queued);
        let (status, _) = h
            .call("POST", "/api/v1/claims", common::WORKER_KEY, body.clone())
            .await?;
        assert!(!status.is_success());
    }
    let failed = repository::get_job(&h.pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert!(failed.is_retryable());
    Ok(())
}

#[tokio::test]
async fn a_worker_that_calls_is_never_the_offline_target_of_its_own_relay() -> anyhow::Result<()> {
    let h = harness().await?;
    let job = queued_job(&h.pool, "film.mkv").await?;
    let worker = h.register(None, common::WORKER_KEY).await?;
    let origin = repository::register(&h.pool, "origine").await?;
    let (_, lease) = common::leases::claim(&h.pool, origin.id).await?;
    let request = RelayRequest {
        target: RelayTarget::Worker { worker_id: worker },
        resume_node_id: "suite".into(),
        variables: Default::default(),
        node_outputs: Default::default(),
    };
    repository::relay_job(&h.pool, job, &lease, &request, false).await?;
    let protocol = LeaseProtocol::LATEST.number();
    h.claim(worker, Some(protocol)).await?;
    silent_for(&h.pool, job, UNREACHABLE_AFTER_SECONDS + 1).await?;
    sqlx::query("UPDATE workers SET last_seen=datetime('now', '-1 day') WHERE id=?1")
        .bind(worker)
        .execute(&h.pool)
        .await?;
    let body = json!({"worker_id": worker, "lease_protocol": protocol, "active_jobs": []});
    let (status, claim) = h
        .call("POST", "/api/v1/claims", common::WORKER_KEY, body)
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claim["job_id"], json!(job));
    Ok(())
}
