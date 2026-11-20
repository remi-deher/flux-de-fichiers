use super::*;
use crate::{
    api::transfers::tests::setup,
    domain::JobErrorCode,
    storage,
    worker_transfer::{TransferState, transfer_router},
};
use forgearr_shared::transfer::{
    ArtifactDeclaration, SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
};
use std::{sync::Arc, time::Duration};

type Publishing = tokio::task::JoinHandle<AppResult<bool>>;

fn ready_request() -> ResultReady {
    ResultReady {
        extension: "mkv".into(),
        warning: false,
    }
}

/// Le worker qui traite annonce une adresse de transfert et déclare le résultat qu'il retient.
async fn retain_result(
    state: &AppState,
    job: &Job,
    lease: &LeaseRequest,
) -> anyhow::Result<ArtifactDeclaration> {
    let url = Some("http://traitant.lan:9000");
    repository::set_transfer_url(&state.pool, lease.worker_id, url).await?;
    let declaration = ArtifactDeclaration {
        root: "/work".into(),
        name: "result-file.mkv".into(),
        sha256: "cd".repeat(32),
        bytes: 6,
    };
    let max = state.config.max_file_bytes;
    let kind = ArtifactKind::Result;
    repository::declare_artifact(&state.pool, job.id, lease, kind, &declaration, max).await?;
    Ok(declaration)
}

/// Lance la publication ; rend la demande que reçoit le détenteur de la bibliothèque.
async fn start_publication(
    state: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    holder: Uuid,
) -> anyhow::Result<(Publishing, SourcePublicationTask)> {
    let (task_state, task_job, task_lease) = (state.clone(), job.clone(), lease.clone());
    let publishing = tokio::spawn(async move {
        publish_pulled(&task_state, &task_job, &task_lease, &ready_request()).await
    });
    let mut tasks = state
        .source_publication
        .take_for_worker(holder, Duration::from_secs(5))
        .await;
    anyhow::ensure!(tasks.len() == 1, "une demande est attendue");
    Ok((publishing, tasks.remove(0)))
}

/// Le détenteur répond à la demande.
async fn answer(
    state: &AppState,
    holder: Uuid,
    task: &SourcePublicationTask,
    outcome: SourcePublicationOutcome,
) -> anyhow::Result<()> {
    let report = SourcePublicationReport {
        request_id: task.request_id,
        worker_id: holder,
        outcome,
    };
    anyhow::ensure!(state.source_publication.report(report).await);
    Ok(())
}

#[tokio::test]
async fn the_holder_pulls_the_retained_result_and_no_central_copy_is_kept() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let declaration = retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;

    // La demande porte le billet pour tirer le résultat chez le worker qui le retient.
    assert_eq!(task.result_sha256, declaration.sha256);
    assert_eq!(task.result_bytes, declaration.bytes);
    let ticket = task
        .result_ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("un billet est attendu"))?;
    assert_eq!(ticket.url, "http://traitant.lan:9000");
    let access = state
        .source_publication
        .access_for_worker(task.request_id, holder)
        .await
        .ok_or_else(|| anyhow::anyhow!("le détenteur doit avoir accès à sa demande"))?;
    assert!(access.staged.is_none(), "le central ne tient aucun fichier");

    answer(&state, holder, &task, SourcePublicationOutcome::Published).await?;
    assert!(publishing.await??);
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    let artifact = repository::get_artifact(&state.pool, job.id, ArtifactKind::Result).await?;
    assert!(artifact.is_none(), "le résultat n'a plus à être retenu");
    let copies = std::fs::read_dir(state.config.data_dir.join("output"))?.count();
    assert_eq!(copies, 0, "le central ne garde aucune copie");
    Ok(())
}

#[tokio::test]
async fn the_ticket_is_valid_only_at_the_worker_retaining_the_result() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let declaration = retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    let ticket = task
        .result_ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("un billet est attendu"))?;
    let grant = repository::redeem_ticket(&state.pool, &ticket.token, lease.worker_id).await?;
    assert_eq!(grant.root, "/work");
    assert_eq!(grant.source_name, declaration.name);
    let elsewhere = repository::redeem_ticket(&state.pool, &ticket.token, holder).await;
    assert!(matches!(elsewhere, Err(AppError::Unauthorized)));
    answer(&state, holder, &task, SourcePublicationOutcome::Published).await?;
    assert!(publishing.await??);
    Ok(())
}

#[tokio::test]
async fn a_changed_source_refuses_the_publication_and_keeps_the_result() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    answer(
        &state,
        holder,
        &task,
        SourcePublicationOutcome::SourceChanged,
    )
    .await?;
    assert!(matches!(publishing.await?, Err(AppError::SourceChanged)));
    let running = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(running.status, JobStatus::Running);
    let artifact = repository::get_artifact(&state.pool, job.id, ArtifactKind::Result).await?;
    assert!(artifact.is_some(), "le worker garde son résultat");
    let journal: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM source_publication_journal")
        .fetch_one(&state.pool)
        .await?;
    assert_eq!(journal, 0, "aucune intention ne reste inscrite");
    Ok(())
}

#[tokio::test]
async fn an_unanswered_publication_is_uncertain_and_never_reported_as_a_success()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    answer(&state, holder, &task, SourcePublicationOutcome::Failed).await?;
    assert!(publishing.await?.is_err());
    let failed = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    let code = Some(JobErrorCode::SourcePublicationUncertain);
    assert_eq!(failed.error_code, code);
    Ok(())
}

#[tokio::test]
async fn other_cases_keep_the_upload_to_the_central() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    // Le worker qui traite est lui-même le détenteur : rien à tirer.
    let itself = pulled_destination(&state, &job, holder).await?;
    assert!(itself.is_none());
    // Dossier de sortie du central : l'envoi au central reste le chemin.
    sqlx::query("UPDATE libraries SET output_policy=?1")
        .bind(LibraryOutputPolicy::OutputDir)
        .execute(&state.pool)
        .await?;
    assert!(!publish_pulled(&state, &job, &lease, &ready_request()).await?);
    let released = repository::get_artifact(&state.pool, job.id, ArtifactKind::Result).await?;
    assert!(released.is_none(), "le fichier retenu n'a plus d'objet");
    Ok(())
}

#[tokio::test]
async fn a_result_must_be_declared_by_the_lease_holder_with_a_valid_extension() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    let undeclared = publish_pulled(&state, &job, &lease, &ready_request()).await;
    assert!(matches!(undeclared, Err(AppError::Invalid)));
    retain_result(&state, &job, &lease).await?;
    let odd = ResultReady {
        extension: "m/v".into(),
        warning: false,
    };
    let invalid = publish_pulled(&state, &job, &lease, &odd).await;
    assert!(matches!(invalid, Err(AppError::Invalid)));
    Ok(())
}

/// La source est tenue par le central (inscriptible) : le worker intégré en est le détenteur.
async fn make_source_central_held(state: &AppState, job: &Job) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE jobs SET source_key=NULL, source_worker_id=NULL, source_writable=1 WHERE id=?1",
    )
    .bind(job.id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// Serveur de transfert du worker qui retient le résultat : il valide les billets auprès du central.
async fn serve_retained_result(state: &AppState, processor: Uuid) -> anyhow::Result<String> {
    let pool = state.pool.clone();
    let transfers = TransferState::new(Arc::new(move |token: String| {
        let pool = pool.clone();
        Box::pin(async move {
            repository::redeem_ticket(&pool, &token, processor)
                .await
                .map_err(|_| axum::http::StatusCode::FORBIDDEN)
        })
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, transfer_router(transfers)).await;
    });
    Ok(format!("http://{address}"))
}

#[tokio::test]
async fn central_sources_go_to_the_integrated_worker_when_active() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    make_source_central_held(&state, &job).await?;
    // Sans worker intégré actif, l'envoi au central reste le chemin.
    let without = pulled_destination(&state, &job, lease.worker_id).await?;
    assert!(without.is_none());
    let local = repository::register(&state.pool, "central-local").await?;
    assert!(state.local_worker_id.set(local.id).is_ok());
    let (policy, holder) = pulled_destination(&state, &job, lease.worker_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("le worker intégré doit être le détenteur"))?;
    assert!(policy == LibraryOutputPolicy::ReplaceWithBackup);
    assert_eq!(holder.worker_id, local.id);
    assert_eq!(holder.root, dir.path().to_string_lossy());
    // Le worker intégré qui traite lui-même n'a rien à tirer.
    let itself = pulled_destination(&state, &job, local.id).await?;
    assert!(itself.is_none());
    Ok(())
}

/// Enregistre le worker intégré et lui donne le staging du central.
async fn activate_integrated_worker(state: &AppState) -> anyhow::Result<Uuid> {
    let local = repository::register(&state.pool, "central-local").await?;
    assert!(state.local_worker_id.set(local.id).is_ok());
    // Le central crée son dossier de staging au démarrage ; le worker intégré y tire le fichier.
    std::fs::create_dir_all(state.config.data_dir.join("staging"))?;
    tokio::spawn(crate::local_worker::respond_to_publications(
        state.clone(),
        local.id,
    ));
    Ok(local.id)
}

/// Le worker qui traite retient le résultat « nouveau » et le sert.
async fn retain_served_result(
    state: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    let retained = dir.join("retained");
    std::fs::create_dir_all(&retained)?;
    let result_file = retained.join("result-file.mkv");
    std::fs::write(&result_file, b"nouveau")?;
    let (sha256, bytes) = storage::fingerprint(&result_file, state.config.max_file_bytes).await?;
    let url = serve_retained_result(state, lease.worker_id).await?;
    repository::set_transfer_url(&state.pool, lease.worker_id, Some(&url)).await?;
    let declaration = ArtifactDeclaration {
        root: retained.to_string_lossy().into_owned(),
        name: "result-file.mkv".into(),
        sha256,
        bytes: i64::try_from(bytes)?,
    };
    let max = state.config.max_file_bytes;
    let kind = ArtifactKind::Result;
    repository::declare_artifact(&state.pool, job.id, lease, kind, &declaration, max).await?;
    Ok(())
}

#[tokio::test]
async fn the_integrated_worker_applies_the_pulled_result() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    make_source_central_held(&state, &job).await?;
    activate_integrated_worker(&state).await?;
    retain_served_result(&state, &job, &lease, dir.path()).await?;
    assert!(publish_pulled(&state, &job, &lease, &ready_request()).await?);
    assert_eq!(std::fs::read(dir.path().join("film.mkv"))?, b"nouveau");
    assert_eq!(std::fs::read(dir.path().join("film.mkv.bak"))?, b"original");
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    let copies = std::fs::read_dir(state.config.data_dir.join("output"))?.count();
    assert_eq!(copies, 0, "le central ne garde aucune copie");
    Ok(())
}

#[tokio::test]
async fn the_integrated_worker_installs_the_result_in_the_output_dir() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    sqlx::query("UPDATE libraries SET output_policy=?1")
        .bind(LibraryOutputPolicy::OutputDir)
        .execute(&state.pool)
        .await?;
    activate_integrated_worker(&state).await?;
    retain_served_result(&state, &job, &lease, dir.path()).await?;
    assert!(publish_pulled(&state, &job, &lease, &ready_request()).await?);
    let name = storage::output_file_name(job.id, lease.lease_token, "mkv");
    let output = state.config.data_dir.join("output").join(name);
    assert_eq!(std::fs::read(output)?, b"nouveau");
    assert_eq!(std::fs::read(dir.path().join("film.mkv"))?, b"original");
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    Ok(())
}
