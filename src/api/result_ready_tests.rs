use super::*;
use crate::{api::transfers::tests::setup, domain::JobErrorCode};
use forgearr_shared::transfer::{
    ArtifactDeclaration, SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
};
use std::time::Duration;

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
    let itself = pulled_policy(&state, &job, holder).await?;
    assert!(itself.is_none());
    // Dossier de sortie du central : l'envoi au central reste le chemin.
    sqlx::query("UPDATE libraries SET output_policy=?1")
        .bind(LibraryOutputPolicy::OutputDir)
        .execute(&state.pool)
        .await?;
    assert!(!publish_pulled(&state, &job, &lease, &ready_request()).await?);
    let still = repository::get_artifact(&state.pool, job.id, ArtifactKind::Result).await?;
    assert!(still.is_some());
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
