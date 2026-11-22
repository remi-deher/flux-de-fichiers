use super::*;
use crate::{
    api::transfers::tests::setup,
    api_keys,
    auth::PrincipalKind,
    domain::{JobEvent, JobEventType, JobStatus},
};
use axum::{
    body::BodyDataStream,
    http::{HeaderValue, header},
    response::IntoResponse,
};
use forgearr_shared::{Permission, Role};
use futures_util::StreamExt;

const LIMIT: Duration = Duration::from_secs(5);
const FAST_CHECK: Duration = Duration::from_millis(20);
/// Plus que la capacité du canal : un abonné qui ne lit pas prend du retard.
const FLOOD: usize = 400;

fn principal(role: Role, scopes: Option<Vec<Permission>>) -> Principal {
    Principal {
        kind: PrincipalKind::ApiKey,
        id: "1".into(),
        name: "test".into(),
        role,
        scopes,
        session_hash: None,
    }
}

fn job_event() -> JobEvent {
    JobEvent {
        event_type: JobEventType::JobCreated,
        job_id: uuid::Uuid::new_v4(),
        status: JobStatus::Queued,
        timestamp: chrono::Utc::now(),
        progress: None,
        message: None,
        error_code: None,
    }
}

async fn stream(
    state: &AppState,
    headers: HeaderMap,
    caller: Principal,
    scope: Scope,
    check_every: Duration,
) -> AppResult<BodyDataStream> {
    let never = std::future::pending();
    let sse = open(state.clone(), headers, caller, scope, check_every, never).await?;
    Ok(sse.into_response().into_body().into_data_stream())
}

fn resuming_from(id: u64) -> HeaderMap {
    HeaderMap::from_iter([(
        header::HeaderName::from_static("last-event-id"),
        HeaderValue::from(id),
    )])
}

/// Lit le flux jusqu'à `needle`.
async fn read_until(body: &mut BodyDataStream, needle: &str) -> anyhow::Result<String> {
    let mut text = String::new();
    while !text.contains(needle) {
        let chunk = tokio::time::timeout(LIMIT, body.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("flux fermé : {text}"))??;
        text.push_str(std::str::from_utf8(&chunk)?);
    }
    Ok(text)
}

#[test]
fn each_caller_receives_only_what_it_can_read() {
    let admin = principal(Role::Admin, None);
    let worker = principal(Role::Worker, None);
    let plugins_only = principal(Role::Mcp, Some(vec![Permission::ViewPlugins]));
    let job = LiveEvent::Job(job_event());
    assert!(Scope::All.admits(&admin) && Scope::Jobs.admits(&admin));
    assert!(!Scope::All.admits(&worker) && !Scope::Jobs.admits(&worker));
    assert!(Scope::All.admits(&plugins_only) && !Scope::Jobs.admits(&plugins_only));
    assert!(!Scope::All.delivers(&plugins_only, &job));
    for (topic, expected) in [
        (ChangeTopic::Catalog, true),
        (ChangeTopic::Tools, true),
        (ChangeTopic::Repositories, true),
        (ChangeTopic::Settings, false),
    ] {
        let event = LiveEvent::Changed(topic);
        assert_eq!(Scope::All.delivers(&plugins_only, &event), expected);
        assert!(Scope::All.delivers(&admin, &event));
        // Le flux des tâches ne transporte aucun changement.
        assert!(!Scope::Jobs.delivers(&admin, &event));
    }
}

#[tokio::test]
async fn the_single_stream_carries_tasks_and_changes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let admin = principal(Role::Admin, None);
    let mut all = stream(&state, HeaderMap::new(), admin.clone(), Scope::All, LIMIT).await?;
    let mut jobs = stream(&state, HeaderMap::new(), admin, Scope::Jobs, LIMIT).await?;
    let changed = LiveEvent::Changed(ChangeTopic::Catalog);
    state.event_history.publish(changed).await;
    state.event_history.publish(job_event()).await;
    let text = read_until(&mut all, "event: job_created").await?;
    assert!(text.contains("event: changed\ndata: {\"topic\":\"catalog\"}"));
    let text = read_until(&mut jobs, "event: job_created").await?;
    assert!(!text.contains(CHANGED_EVENT) && !text.contains(RESYNC_EVENT));
    Ok(())
}

#[tokio::test]
async fn a_position_of_a_previous_instance_gets_one_resync_then_the_new_events()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    for scope in [Scope::All, Scope::Jobs] {
        let admin = principal(Role::Admin, None);
        let mut body = stream(&state, resuming_from(u64::MAX), admin, scope, LIMIT).await?;
        read_until(&mut body, RESYNC_EVENT).await?;
        state.event_history.publish(job_event()).await;
        let text = read_until(&mut body, "event: job_created").await?;
        assert!(!text.contains(RESYNC_EVENT));
    }
    Ok(())
}

#[tokio::test]
async fn a_late_subscriber_is_told_to_reread_everything() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let admin = principal(Role::Admin, None);
    let mut body = stream(&state, HeaderMap::new(), admin, Scope::Jobs, LIMIT).await?;
    for _ in 0..FLOOD {
        state.event_history.publish(job_event()).await;
    }
    let text = read_until(&mut body, RESYNC_EVENT).await?;
    assert!(!text.contains("event: job_created"));
    read_until(&mut body, "event: job_created").await?;
    Ok(())
}

#[tokio::test]
async fn a_caller_without_any_view_right_is_refused() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let worker = principal(Role::Worker, None);
    let refused = stream(&state, HeaderMap::new(), worker, Scope::All, LIMIT).await;
    assert!(matches!(refused, Err(AppError::Forbidden)));
    Ok(())
}

#[tokio::test]
async fn a_revoked_caller_loses_its_stream() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let key = api_keys::issue(&state.pool, "flux", Role::Viewer).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", key.raw))?,
    );
    let caller = authenticate_request(&state, &headers, &Method::GET)
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let mut body = stream(&state, headers, caller, Scope::All, FAST_CHECK).await?;
    // Toujours servi tant que la clé vaut.
    tokio::time::sleep(FAST_CHECK * 3).await;
    state.event_history.publish(job_event()).await;
    read_until(&mut body, "event: job_created").await?;
    crate::repository::revoke_api_key(&state.pool, &key.record.id).await?;
    let closed = tokio::time::timeout(LIMIT, async { while body.next().await.is_some() {} });
    closed.await?;
    Ok(())
}

#[tokio::test]
async fn a_stream_ends_when_the_server_stops() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let (stop, stopping) = tokio::sync::oneshot::channel::<()>();
    let until = async move {
        let _ = stopping.await;
    };
    let admin = principal(Role::Admin, None);
    let sse = open(
        state.clone(),
        HeaderMap::new(),
        admin,
        Scope::All,
        LIMIT,
        until,
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let mut body = sse.into_response().into_body().into_data_stream();
    state.event_history.publish(job_event()).await;
    read_until(&mut body, "event: job_created").await?;
    let _ = stop.send(());
    let closed = tokio::time::timeout(LIMIT, async { while body.next().await.is_some() {} });
    closed.await?;
    Ok(())
}
