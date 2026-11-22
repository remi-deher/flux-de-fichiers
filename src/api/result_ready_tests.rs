use super::*;
use crate::{
    api::transfers::tests::{current_protocol, setup},
    domain::JobErrorCode,
    storage,
    worker_transfer::{TransferState, transfer_router},
};
use forgearr_shared::transfer::{
    ArtifactDeclaration, SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
};
use std::time::Duration;

type Publishing = tokio::task::JoinHandle<AppResult<()>>;

fn ready_request() -> ResultReady {
    ResultReady {
        extension: "mkv".into(),
        warning: false,
        can_apply_locally: false,
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
    assert_eq!(access.job_id, job.id);

    answer(&state, holder, &task, SourcePublicationOutcome::Published).await?;
    publishing.await??;
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    assert!(
        done.output_name.is_none(),
        "le résultat remplacé n'a pas de nom"
    );
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
    publishing.await??;
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
    assert!(repository::authorize_source_publication(&state.pool, job.id, &lease).await?);
    answer(&state, holder, &task, SourcePublicationOutcome::Failed).await?;
    assert!(publishing.await?.is_err());
    let failed = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    let code = Some(JobErrorCode::SourcePublicationUncertain);
    assert_eq!(failed.error_code, code);
    assert!(
        failed.output_name.is_none(),
        "aucune copie à récupérer au central"
    );
    let journal: (String, String) = sqlx::query_as(
        "SELECT output_name, output_extension FROM source_publication_journal WHERE job_id=?1",
    )
    .bind(job.id)
    .fetch_one(&state.pool)
    .await?;
    assert_eq!(journal, (String::new(), "mkv".to_owned()));
    Ok(())
}

#[tokio::test]
async fn without_a_reachable_holder_the_result_is_refused_and_kept() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    // Le worker qui traite est lui-même le détenteur : rien à tirer.
    let itself = pulled_destination(&state, &job, holder).await?;
    assert!(itself.is_none());
    // Dossier de sortie du central sans worker intégré actif : refus explicite.
    sqlx::query("UPDATE libraries SET output_policy=?1")
        .bind(LibraryOutputPolicy::OutputDir)
        .execute(&state.pool)
        .await?;
    let refused = publish_pulled(&state, &job, &lease, &ready_request()).await;
    assert!(matches!(refused, Err(AppError::ResultUndeliverable)));
    let kept = repository::get_artifact(&state.pool, job.id, ArtifactKind::Result).await?;
    assert!(kept.is_some(), "le résultat reste retenu");
    Ok(())
}

#[tokio::test]
async fn a_processor_without_address_is_refused_before_any_journal() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    repository::set_transfer_url(&state.pool, lease.worker_id, None).await?;
    let refused = publish_pulled(&state, &job, &lease, &ready_request()).await;
    assert!(matches!(refused, Err(AppError::ResultUndeliverable)));
    assert!(pending_journal(&state).await?.is_empty());
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
        ..ready_request()
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
    let input_dir = state.config.input_dir.clone();
    let redeemer = crate::local_worker::local_redeemer(state.pool.clone(), processor, input_dir);
    let transfers = TransferState::new(redeemer);
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
    publish_pulled(&state, &job, &lease, &ready_request()).await?;
    assert_eq!(std::fs::read(dir.path().join("film.mkv"))?, b"nouveau");
    assert_eq!(std::fs::read(dir.path().join("film.mkv.bak"))?, b"original");
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    let copies = std::fs::read_dir(state.config.data_dir.join("output"))?.count();
    assert_eq!(copies, 0, "le central ne garde aucune copie");
    Ok(())
}

#[tokio::test]
async fn the_local_redeemer_refuses_an_unknown_ticket() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, lease, _) = setup(dir.path()).await?;
    let input_dir = state.config.input_dir.clone();
    let pool = state.pool.clone();
    let redeemer = crate::local_worker::local_redeemer(pool, lease.worker_id, input_dir);
    let refused = redeemer("inconnu".to_owned()).await;
    assert_eq!(refused.err(), Some(axum::http::StatusCode::FORBIDDEN));
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
    publish_pulled(&state, &job, &lease, &ready_request()).await?;
    let name = storage::output_file_name(job.id, lease.lease_token, "mkv");
    let output = state.config.data_dir.join("output").join(name);
    assert_eq!(std::fs::read(output)?, b"nouveau");
    assert_eq!(std::fs::read(dir.path().join("film.mkv"))?, b"original");
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    Ok(())
}

/// Le détenteur de la source traite lui-même la tâche : il retient son résultat et l'annonce.
async fn processing_holder(
    state: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    holder: Uuid,
) -> anyhow::Result<LeaseRequest> {
    sqlx::query("UPDATE jobs SET worker_id=?2 WHERE id=?1")
        .bind(job.id)
        .bind(holder)
        .execute(&state.pool)
        .await?;
    let own = LeaseRequest {
        worker_id: holder,
        ..lease.clone()
    };
    retain_result(state, job, &own).await?;
    Ok(own)
}

async fn pending_journal(state: &AppState) -> anyhow::Result<Vec<(String, String)>> {
    let pending = repository::pending_source_publications(&state.pool).await?;
    let entries = pending
        .into_iter()
        .map(|p| (p.output_name, p.output_extension))
        .collect();
    Ok(entries)
}

#[tokio::test]
async fn a_holding_worker_applies_the_result_itself_and_confirms() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let own = processing_holder(&state, &job, &lease, holder).await?;
    let local = begin_local_application(&state, &job, &own, &ready_request())
        .await?
        .ok_or_else(|| anyhow::anyhow!("le détenteur doit appliquer lui-même"))?;
    assert!(local.policy == LibraryOutputPolicy::ReplaceWithBackup);
    assert_eq!(local.root, dir.path().to_string_lossy());
    assert_eq!(
        pending_journal(&state).await?,
        [(String::new(), "mkv".to_owned())]
    );

    // La réponse à l'annonce s'était perdue : la répétition retrouve l'intention inscrite.
    let again = begin_local_application(&state, &job, &own, &ready_request()).await?;
    assert!(again.is_some());

    let published = SourcePublicationOutcome::Published;
    finish_local_application(&state, &job, &own, published).await?;
    let repeated = repeated_outcome(&state, job.id, &own, published).await?;
    assert!(matches!(repeated, Some(Ok(_))));
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    assert!(done.output_name.is_none(), "aucune copie au central");
    assert!(pending_journal(&state).await?.is_empty());
    let artifact = repository::get_artifact(&state.pool, job.id, ArtifactKind::Result).await?;
    assert!(artifact.is_none(), "le résultat n'a plus à être retenu");
    Ok(())
}

#[tokio::test]
async fn a_changed_source_or_a_failure_closes_the_local_application() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let own = processing_holder(&state, &job, &lease, holder).await?;
    begin_local_application(&state, &job, &own, &ready_request()).await?;
    let changed = SourcePublicationOutcome::SourceChanged;
    let refused = finish_local_application(&state, &job, &own, changed).await;
    assert!(matches!(refused, Err(AppError::SourceChanged)));
    assert!(pending_journal(&state).await?.is_empty());
    let closed = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(closed.status, JobStatus::Failed);
    assert_eq!(closed.error_code, Some(JobErrorCode::SourceChanged));
    // Le rapport répété (réponse perdue) retrouve la même issue.
    let repeated = repeated_outcome(&state, job.id, &own, changed).await?;
    assert!(matches!(repeated, Some(Err(AppError::SourceChanged))));

    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let own = processing_holder(&state, &job, &lease, holder).await?;
    begin_local_application(&state, &job, &own, &ready_request()).await?;
    assert!(repository::authorize_source_publication(&state.pool, job.id, &own).await?);
    let failed = SourcePublicationOutcome::Failed;
    let unknown = finish_local_application(&state, &job, &own, failed).await;
    assert!(unknown.is_err());
    let uncertain = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(uncertain.status, JobStatus::Failed);
    let code = Some(JobErrorCode::SourcePublicationUncertain);
    assert_eq!(uncertain.error_code, code);
    Ok(())
}

#[tokio::test]
async fn only_the_holder_applies_locally_after_the_journal() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    // Le worker qui traite ne détient pas la source : un autre détenteur la tire.
    let elsewhere = begin_local_application(&state, &job, &lease, &ready_request()).await?;
    assert!(elsewhere.is_none());
    // Sans inscription au journal, aucune confirmation n'est acceptée.
    let published = SourcePublicationOutcome::Published;
    let early = finish_local_application(&state, &job, &lease, published).await;
    assert!(matches!(early, Err(AppError::Conflict)));
    Ok(())
}

/// Le worker intégré traite la tâche à la place du worker de calcul, avec ou sans adresse annoncée.
async fn integrated_processor(
    state: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    url: Option<&str>,
) -> anyhow::Result<LeaseRequest> {
    let local = repository::register(&state.pool, "central-local").await?;
    repository::set_transfer_url(&state.pool, local.id, url).await?;
    sqlx::query("UPDATE jobs SET worker_id=?2 WHERE id=?1")
        .bind(job.id)
        .bind(local.id)
        .execute(&state.pool)
        .await?;
    Ok(LeaseRequest {
        worker_id: local.id,
        ..lease.clone()
    })
}

fn work_file(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let work = dir.join("work");
    std::fs::create_dir_all(&work)?;
    let file = work.join("out.mkv");
    std::fs::write(&file, b"nouveau")?;
    Ok(file)
}

#[tokio::test]
async fn the_integrated_worker_retains_its_result_for_a_remote_holder() -> anyhow::Result<()> {
    use crate::local_worker::retention::publish_retained;
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let url = Some("http://central.lan:9000");
    let own = integrated_processor(&state, &job, &lease, url).await?;
    let file = work_file(dir.path())?;
    let (task_state, task_job, task_lease) = (state.clone(), job.clone(), own.clone());
    let publishing = tokio::spawn(async move {
        let ready = ready_request();
        publish_retained(&task_state, &task_job, &task_lease, &file, &ready).await
    });
    let mut tasks = state
        .source_publication
        .take_for_worker(holder, Duration::from_secs(5))
        .await;
    anyhow::ensure!(tasks.len() == 1, "une demande est attendue");
    let task = tasks.remove(0);
    let ticket = task.result_ticket.clone();
    assert_eq!(
        ticket.map(|t| t.url).as_deref(),
        Some("http://central.lan:9000")
    );
    let kind = ArtifactKind::Result;
    let artifact = repository::get_artifact(&state.pool, job.id, kind).await?;
    let retained_dir = state.config.data_dir.join("retained");
    let root = artifact.map(|a| a.declaration.root);
    assert_eq!(
        root.as_deref(),
        Some(retained_dir.to_string_lossy().as_ref())
    );

    answer(&state, holder, &task, SourcePublicationOutcome::Published).await?;
    assert!(publishing.await??);
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    let copies = std::fs::read_dir(&retained_dir)?.count();
    assert_eq!(copies, 0, "copie purgée");
    Ok(())
}

#[tokio::test]
async fn the_integrated_worker_publishes_itself_when_nobody_can_pull() -> anyhow::Result<()> {
    use crate::local_worker::retention::publish_retained;
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    let file = work_file(dir.path())?;
    // Sans adresse de transfert : personne ne pourrait tirer le résultat chez lui.
    let silent = integrated_processor(&state, &job, &lease, None).await?;
    let ready = ready_request();
    assert!(!publish_retained(&state, &job, &silent, &file, &ready).await?);
    // Source du central : le résultat s'applique sur place.
    let url = Some("http://central.lan:9000");
    repository::set_transfer_url(&state.pool, silent.worker_id, url).await?;
    make_source_central_held(&state, &job).await?;
    assert!(state.local_worker_id.set(silent.worker_id).is_ok());
    assert!(!publish_retained(&state, &job, &silent, &file, &ready).await?);
    let kind = ArtifactKind::Result;
    let artifact = repository::get_artifact(&state.pool, job.id, kind).await?;
    assert!(artifact.is_none(), "rien de retenu");
    Ok(())
}

#[tokio::test]
async fn the_integrated_worker_retains_a_working_file_to_relay_the_task() -> anyhow::Result<()> {
    use crate::local_worker::retention::relay_working_file;
    use forgearr_shared::relay::{RelayRequest, RelayTarget};
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let file = work_file(dir.path())?;
    let work_dir = dir.path().join("work");
    let request = RelayRequest {
        target: RelayTarget::Worker { worker_id: holder },
        resume_node_id: "n2".into(),
        variables: Default::default(),
        node_outputs: Default::default(),
    };
    // Sans adresse de transfert, personne ne pourrait tirer le fichier de travail.
    let silent = integrated_processor(&state, &job, &lease, None).await?;
    let working = (file.as_path(), work_dir.as_path());
    let refused = relay_working_file(&state, job.id, &silent, &request, working, false).await;
    assert!(refused.is_err());

    let url = Some("http://central.lan:9000");
    repository::set_transfer_url(&state.pool, silent.worker_id, url).await?;
    relay_working_file(&state, job.id, &silent, &request, working, false).await?;
    let queued = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.target_worker_id, Some(holder));
    let kind = ArtifactKind::Working;
    let artifact = repository::get_artifact(&state.pool, job.id, kind).await?;
    assert!(artifact.is_some(), "le fichier de travail reste retenu");
    Ok(())
}

#[tokio::test]
async fn a_pulled_publication_cannot_be_concluded_by_the_processing_worker() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    // Un autre détenteur tire et applique : le worker qui traite ne peut ni autoriser ni conclure.
    let published = SourcePublicationOutcome::Published;
    let early = finish_local_application(&state, &job, &lease, published).await;
    assert!(matches!(early, Err(AppError::Conflict)));
    let running = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(running.status, JobStatus::Running);

    answer(&state, holder, &task, SourcePublicationOutcome::Published).await?;
    publishing.await??;
    Ok(())
}

#[tokio::test]
async fn after_a_central_restart_a_repeated_announce_never_publishes_twice() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    // Le détenteur obtient l'accord, puis le central redémarre : la demande en mémoire est perdue.
    assert!(repository::authorize_source_publication(&state.pool, job.id, &lease).await?);
    publishing.abort();
    let _ = publishing.await;
    let announced = publish_announced(state.clone(), job.id, lease.clone(), ready_request()).await;
    assert!(matches!(announced, Err(AppError::PublicationPending)));
    let offered = state
        .source_publication
        .take_for_worker(holder, Duration::ZERO)
        .await;
    assert!(offered.is_empty(), "aucune seconde exécution");
    let report = SourcePublicationReport {
        request_id: task.request_id,
        worker_id: holder,
        outcome: SourcePublicationOutcome::SourceChanged,
    };
    assert!(!state.source_publication.report(report).await);
    assert_eq!(pending_journal(&state).await?.len(), 1);
    // La reprise conclut par un succès : la répétition suivante le constate.
    let output = PublishedOutput {
        name: "",
        sha256: &task.result_sha256,
        bytes: task.result_bytes,
        warning: false,
    };
    repository::publish_succeeded(&state.pool, job.id, &lease, output, async { Ok(()) }).await?;
    publish_announced(state.clone(), job.id, lease.clone(), ready_request()).await?;
    Ok(())
}

#[tokio::test]
async fn an_intent_never_authorized_is_dropped_and_announced_again() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, _) = start_publication(&state, &job, &lease, holder).await?;
    publishing.abort();
    let _ = publishing.await;
    let epoch = lease.lease_epoch;
    assert!(repository::abandon_unauthorized_publication(&state.pool, job.id, epoch).await?);
    // Plus d'intention : plus d'accord possible pour l'exécutant disparu.
    assert!(!repository::authorize_source_publication(&state.pool, job.id, &lease).await?);
    let (again, task) = start_publication(&state, &job, &lease, holder).await?;
    answer(&state, holder, &task, SourcePublicationOutcome::Published).await?;
    again.await??;
    let done = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(done.status, JobStatus::Succeeded);
    Ok(())
}

#[tokio::test]
async fn a_failure_before_the_grant_leaves_the_lease_to_its_holder() -> anyhow::Result<()> {
    let failed = SourcePublicationOutcome::Failed;
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    answer(&state, holder, &task, failed).await?;
    assert!(publishing.await?.is_err());
    assert!(pending_journal(&state).await?.is_empty());
    // Le titulaire déclare un échec ordinaire.
    let code = JobErrorCode::TransferFailed;
    repository::fail(&state.pool, job.id, &lease, code).await?;

    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let own = processing_holder(&state, &job, &lease, holder).await?;
    begin_local_application(&state, &job, &own, &ready_request()).await?;
    let refused = finish_local_application(&state, &job, &own, failed).await;
    assert!(refused.is_err());
    assert!(pending_journal(&state).await?.is_empty());
    repository::active(&state.pool, job.id, &own).await?;
    Ok(())
}

#[tokio::test]
async fn a_failure_before_the_grant_requeues_the_job_of_a_restarted_holder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    current_protocol(&state, job.id).await?;
    retain_result(&state, &job, &lease).await?;
    let (publishing, task) = start_publication(&state, &job, &lease, holder).await?;
    let boot = Uuid::new_v4();
    let changes = repository::register_boot(&state.pool, lease.worker_id, boot).await?;
    assert!(changes.iter().all(|change| change.deferred));
    answer(&state, holder, &task, SourcePublicationOutcome::Failed).await?;
    assert!(publishing.await?.is_err());
    let queued = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert!(pending_journal(&state).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_dropped_intent_can_no_longer_be_granted() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let own = processing_holder(&state, &job, &lease, holder).await?;
    begin_local_application(&state, &job, &own, &ready_request()).await?;
    let epoch = own.lease_epoch;
    repository::abandon_unauthorized_publication(&state.pool, job.id, epoch).await?;
    let refused = repository::require_publication_grant(&state.pool, job.id, &own).await;
    assert!(matches!(refused, Err(AppError::Conflict)));
    Ok(())
}

const HOLDER_KEY: &str = "fdf_d0000001_0123456789abcdef0123456789abcdef0123456789abcdef";

/// En-têtes du titulaire `own`, enregistré sous le nom `holder`.
async fn lease_headers(state: &AppState, own: &LeaseRequest) -> anyhow::Result<HeaderMap> {
    let role = forgearr_shared::Role::Worker;
    crate::api_keys::install_known_key(&state.pool, HOLDER_KEY, "holder", role).await?;
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("authorization", format!("Bearer {HOLDER_KEY}")),
        ("x-worker-id", own.worker_id.to_string()),
        ("x-lease-token", own.lease_token.to_string()),
        ("x-lease-epoch", own.lease_epoch.to_string()),
    ] {
        headers.insert(name, value.parse()?);
    }
    Ok(headers)
}

#[tokio::test]
async fn a_repeated_outcome_joins_the_running_conclusion() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let own = processing_holder(&state, &job, &lease, holder).await?;
    begin_local_application(&state, &job, &own, &ready_request()).await?;
    let headers = lease_headers(&state, &own).await?;
    let report = || {
        let published = Json(SourcePublicationOutcome::Published);
        applied(
            State(state.clone()),
            headers.clone(),
            Path(job.id),
            published,
        )
    };
    // Une conclusion court déjà pour ce bail.
    let key = (job.id, own.lease_epoch);
    let (release, gate) = tokio::sync::oneshot::channel::<()>();
    let publications = state.publications.clone();
    let running = tokio::spawn(async move {
        let conclusion = async move {
            let _ = gate.await;
            Ok(())
        };
        publications.run(key, conclusion).await
    });
    while !state.publications.contains(key) {
        tokio::task::yield_now().await;
    }
    let repeated = tokio::spawn(report());
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!repeated.is_finished(), "la répétition attend");
    let _ = release.send(());
    running.await??;
    assert_eq!(repeated.await??, StatusCode::NO_CONTENT);
    let status = repository::get_job(&state.pool, job.id).await?.status;
    assert_eq!(status, JobStatus::Running, "jointe, pas exécutée à part");

    for _ in 0..2 {
        assert_eq!(report().await?, StatusCode::NO_CONTENT);
    }
    let status = repository::get_job(&state.pool, job.id).await?.status;
    assert_eq!(status, JobStatus::Succeeded);
    Ok(())
}

#[tokio::test]
async fn an_announce_renews_an_overdue_lease_before_the_sweep() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 second') WHERE id=?1")
        .bind(job.id)
        .execute(&state.pool)
        .await?;
    // Refusée faute d'artefact déclaré, l'annonce vaut déjà contact.
    let announced = publish_announced(state.clone(), job.id, lease.clone(), ready_request()).await;
    assert!(matches!(announced, Err(AppError::Invalid)));
    repository::sweep(&state.pool, None, &Default::default()).await?;
    repository::active(&state.pool, job.id, &lease).await?;
    Ok(())
}

#[tokio::test]
async fn only_an_unpublished_output_is_discarded() -> anyhow::Result<()> {
    const NAME: &str = "resultat.mkv";
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    let file = state.config.data_dir.join("output").join(NAME);
    std::fs::write(&file, b"nouveau")?;
    discard_output(&state, job.id, NAME).await;
    assert!(!file.exists());
    std::fs::write(&file, b"nouveau")?;
    let output = PublishedOutput {
        name: NAME,
        sha256: &"cd".repeat(32),
        bytes: 7,
        warning: false,
    };
    repository::publish_succeeded(&state.pool, job.id, &lease, output, async { Ok(()) }).await?;
    discard_output(&state, job.id, NAME).await;
    assert!(file.exists(), "sortie d'une tâche réussie");
    Ok(())
}
