//! Exécuteur déclaratif de contrats d'opérations binaires (`probe`, `in-place`, `staged-output`).

use crate::{
    domain::IrreversibleAction,
    flows::contract::{
        BinaryExecutionMode, BinaryRuntime, OperationContract, OperationRegistry, OperationRuntime,
        ToolRef, apply_derived, build_binary_argv, evaluate_condition, merge_params,
    },
    job_runner::action_gate::{ActionGate, ActionRequest},
    media::{
        PostValidateMode, PostValidateSpec, ProbeCommand, ensure_non_empty_file,
        extract_duration_secs, merge_probe_enrichment, run_probe_json, validate_magic_bytes,
        validate_probe_preserved,
    },
    tool_run::{CommandSpec, ToolRunError, run_command},
    tools_cache::{ToolCacheManager, host_arch},
};
use forgearr_shared::jobs::JobErrorCode;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

/// Résultat d'une exécution d'opération binaire déclarative.
#[derive(Clone, Debug)]
pub struct BinaryStepOutcome {
    pub next_working_file: Option<PathBuf>,
    pub updated_media_info: Option<Value>,
    pub warning: bool,
    pub is_staged_output: bool,
    pub modified_in_place: bool,
    pub message: Option<String>,
    pub excerpt: Option<String>,
}

/// Erreur d'exécution d'une opération binaire.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct BinaryStepError {
    pub code: JobErrorCode,
    pub message: String,
    pub excerpt: Option<String>,
}

/// Résout l'exécutable local d'un contrat binaire (cache local -> variable d'environnement -> PATH).
pub fn resolve_contract_binary(
    cache_mgr: &ToolCacheManager,
    rt: &BinaryRuntime,
    tool_versions: &HashMap<String, String>,
) -> PathBuf {
    let env_var = rt.env_override.as_deref().unwrap_or("");
    if let Some(tool) = &rt.tool {
        let version = tool_versions
            .get(&tool.id)
            .map(String::as_str)
            .unwrap_or(&tool.version);
        return cache_mgr.resolve_executable(&tool.id, version, &rt.executable, env_var);
    }
    if !env_var.is_empty()
        && let Ok(env_bin) = std::env::var(env_var)
        && !env_bin.trim().is_empty()
    {
        return PathBuf::from(env_bin);
    }
    PathBuf::from(&rt.executable)
}

/// Vérifie si une sonde d'enrichissement optionnelle est disponible localement sans erreur si absente.
pub fn is_binary_available_locally(
    cache_mgr: &ToolCacheManager,
    rt: &BinaryRuntime,
    tool_versions: &HashMap<String, String>,
) -> bool {
    if let Some(env_var) = rt.env_override.as_deref()
        && !env_var.is_empty()
        && std::env::var_os(env_var).is_some()
    {
        return true;
    }
    if let Some(tool) = &rt.tool {
        let version = tool_versions
            .get(&tool.id)
            .map(String::as_str)
            .unwrap_or(&tool.version);
        if cache_mgr.is_ready(&tool.id, version, host_arch()) {
            return true;
        }
    }
    false
}

/// Lance la sonde déclarée sur `target` et renvoie son JSON validé.
pub async fn run_probe_contract(
    cache_mgr: &ToolCacheManager,
    tool_versions: &HashMap<String, String>,
    op: &OperationContract,
    target: &Path,
) -> Result<Value, String> {
    let OperationRuntime::Binary(rt) = &op.runtime else {
        return Err(format!("contrat « {} » non binaire", op.id));
    };
    let bin = resolve_contract_binary(cache_mgr, rt, tool_versions);
    let paths = HashMap::from([("source", target)]);
    let (args, _) = build_binary_argv(rt, &Value::Null, &paths, None).map_err(|e| e.to_string())?;
    let mut cmd = ProbeCommand::new(&rt.executable, &bin, &args);
    cmd.format = rt.probe_format.unwrap_or_default();
    cmd.require_paths = &rt.probe_require_paths;
    if !rt.console.success_exit_codes.is_empty() {
        cmd.success_exit_codes = &rt.console.success_exit_codes;
    }
    cmd.timeout = Duration::from_secs(rt.console.timeout_seconds.max(1));
    run_probe_json(cmd).await.map_err(|e| e.to_string())
}

/// `PATH` plaçant en tête les dossiers binaires des outils du cache listés, puis `base`
/// (le `PATH` courant si absent) ; `None` sans outil, pour hériter de l'environnement.
pub fn tool_search_path(
    cache_mgr: &ToolCacheManager,
    tool_versions: &HashMap<String, String>,
    tools: &[ToolRef],
    base: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
    if tools.is_empty() {
        return None;
    }
    let arch = host_arch();
    let tool_dirs = tools.iter().map(|tool| {
        let version = tool_versions
            .get(&tool.id)
            .map(String::as_str)
            .unwrap_or(&tool.version);
        cache_mgr.tool_dir(&tool.id, version, arch).join("bin")
    });
    let base = base
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let inherited: Vec<PathBuf> = std::env::split_paths(&base).collect();
    std::env::join_paths(tool_dirs.chain(inherited)).ok()
}

fn enrichment_key<'a>(op: &'a OperationContract, rt: &'a BinaryRuntime) -> &'a str {
    rt.probe_enrichment_key.as_deref().unwrap_or(op.id.as_str())
}

/// Inspecte `source` via la sonde principale et enrichit optionnellement si une sonde `probe-enrich` est prête.
pub async fn probe_media_tree(
    registry: &OperationRegistry,
    cache_mgr: &ToolCacheManager,
    tool_versions: &HashMap<String, String>,
    source: &Path,
) -> Result<Value, BinaryStepError> {
    let primary = registry
        .primary_probe_for_file(source)
        .ok_or_else(|| BinaryStepError {
            code: JobErrorCode::ProbeUnavailable,
            message: format!(
                "aucune sonde primaire déclarée pour '{}'",
                source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("source")
            ),
            excerpt: None,
        })?;
    let mut info = run_probe_contract(cache_mgr, tool_versions, primary, source)
        .await
        .map_err(|err| BinaryStepError {
            code: error_code_for_op(primary, JobErrorCode::OperationFailed),
            message: format!("inspection impossible : {err}"),
            excerpt: None,
        })?;
    for enrich in registry.enrich_probes_for_file(source) {
        if let OperationRuntime::Binary(rt) = &enrich.runtime
            && is_binary_available_locally(cache_mgr, rt, tool_versions)
        {
            match run_probe_contract(cache_mgr, tool_versions, enrich, source).await {
                Ok(data) => {
                    info = merge_probe_enrichment(info, enrichment_key(enrich, rt), data);
                }
                Err(err) => {
                    tracing::warn!(operation = %enrich.id, error = %err, "enrichissement de sonde ignoré");
                }
            }
        }
    }
    Ok(info)
}

fn error_code_for_op(op: &OperationContract, fallback: JobErrorCode) -> JobErrorCode {
    match &op.runtime {
        OperationRuntime::Binary(rt) => rt.error_code.unwrap_or(fallback),
        OperationRuntime::Http(rt) => rt.error_code.unwrap_or(fallback),
        OperationRuntime::Script(rt) => rt.error_code.unwrap_or(fallback),
        OperationRuntime::Primitive(_) => fallback,
    }
}

/// Callback de progression émis pendant l'exécution d'une opération binaire.
pub type ProgressCallback = Arc<dyn Fn(u32, &str) + Send + Sync>;

/// Contexte d'exécution passé à `execute_binary_operation`.
pub struct BinaryExecContext<'a> {
    pub registry: &'a OperationRegistry,
    pub cache_mgr: &'a ToolCacheManager,
    pub tool_versions: &'a HashMap<String, String>,
    pub work_dir: &'a Path,
    pub working_file: &'a Path,
    /// Fichier d'origine de la tâche, exposé aux contrats par le chemin `reference`.
    pub source_file: &'a Path,
    pub media_info: &'a Value,
    pub step_index: i32,
    pub on_progress: ProgressCallback,
    /// Bloc exécuté, et accord à demander avant une opération en place.
    pub node_id: &'a str,
    pub action_gate: Option<&'a dyn ActionGate>,
}

/// Exécute une opération binaire déclarative (`ProbeEnrich`, `InPlace` ou `StagedOutput`).
pub async fn execute_binary_operation(
    ctx: BinaryExecContext<'_>,
    op: &OperationContract,
    rt: &BinaryRuntime,
    node_config: &Value,
) -> Result<BinaryStepOutcome, BinaryStepError> {
    let error_code = rt.error_code.unwrap_or(JobErrorCode::OperationFailed);
    match rt.execution_mode {
        BinaryExecutionMode::ProbePrimary | BinaryExecutionMode::ProbeEnrich => {
            let mut updated = probe_media_tree(
                ctx.registry,
                ctx.cache_mgr,
                ctx.tool_versions,
                ctx.working_file,
            )
            .await?;
            if rt.execution_mode == BinaryExecutionMode::ProbeEnrich {
                let extra =
                    run_probe_contract(ctx.cache_mgr, ctx.tool_versions, op, ctx.working_file)
                        .await
                        .map_err(|err| BinaryStepError {
                            code: error_code,
                            message: format!("ré-inspection média : {err}"),
                            excerpt: None,
                        })?;
                updated = merge_probe_enrichment(updated, enrichment_key(op, rt), extra);
            }
            Ok(BinaryStepOutcome {
                next_working_file: None,
                updated_media_info: Some(updated),
                warning: false,
                is_staged_output: false,
                modified_in_place: false,
                message: rt
                    .success_message
                    .clone()
                    .or_else(|| Some("métadonnées techniques actualisées".to_owned())),
                excerpt: None,
            })
        }
        BinaryExecutionMode::InPlace => {
            let paths =
                HashMap::from([("source", ctx.working_file), ("reference", ctx.source_file)]);
            let (args, has_changes) =
                build_binary_argv(rt, node_config, &paths, None).map_err(|err| {
                    BinaryStepError {
                        code: error_code,
                        message: format!("{} : {err}", rt.executable),
                        excerpt: None,
                    }
                })?;
            if rt.skip_when_unchanged && !has_changes {
                return Ok(BinaryStepOutcome {
                    next_working_file: None,
                    updated_media_info: None,
                    warning: false,
                    is_staged_output: false,
                    modified_in_place: false,
                    message: Some("aucune modification demandée".to_owned()),
                    excerpt: None,
                });
            }
            if let Some(gate) = ctx.action_gate {
                let request = ActionRequest::block(IrreversibleAction::InPlace, ctx.node_id);
                gate.authorize(request)
                    .await
                    .map_err(|refusal| BinaryStepError {
                        code: error_code,
                        message: refusal.message(),
                        excerpt: None,
                    })?;
            }
            let bin = resolve_contract_binary(ctx.cache_mgr, rt, ctx.tool_versions);
            let path_env = tool_search_path(ctx.cache_mgr, ctx.tool_versions, &rt.path_tools, None);
            let spec = command_spec_from_contract(rt, &bin, &args, None, path_env);
            let outcome = run_command(spec, |_, _| {}).await.map_err(|err| {
                let excerpt = extract_excerpt(&err);
                BinaryStepError {
                    code: error_code,
                    message: format!("{} : {err}", rt.executable),
                    excerpt,
                }
            })?;
            post_validate_output(
                ctx.registry,
                ctx.cache_mgr,
                ctx.tool_versions,
                &rt.post_validate_spec(),
                Some(ctx.media_info),
                ctx.working_file,
                ctx.working_file,
            )
            .await
            .map_err(|err| BinaryStepError {
                code: error_code,
                message: format!("validation sortie : {err}"),
                excerpt: outcome.excerpt.clone(),
            })?;
            let msg = select_status_message(rt, outcome.warning, None);
            Ok(BinaryStepOutcome {
                next_working_file: None,
                updated_media_info: None,
                warning: outcome.warning,
                is_staged_output: false,
                modified_in_place: true,
                message: msg,
                excerpt: outcome.excerpt,
            })
        }
        BinaryExecutionMode::StagedOutput => {
            let base_name = rt.output_filename.as_deref().unwrap_or("output.bin");
            let mut destination = ctx.work_dir.join(base_name);
            if destination == ctx.working_file {
                destination = ctx
                    .work_dir
                    .join(format!("step-{}-{base_name}", ctx.step_index));
            }
            let paths = HashMap::from([
                ("source", ctx.working_file),
                ("reference", ctx.source_file),
                ("destination", destination.as_path()),
            ]);
            let path_env = tool_search_path(ctx.cache_mgr, ctx.tool_versions, &rt.path_tools, None);
            let (args, _) = build_binary_argv(rt, node_config, &paths, None).map_err(|err| {
                BinaryStepError {
                    code: error_code,
                    message: format!("{} : {err}", rt.executable),
                    excerpt: None,
                }
            })?;

            let mut duration_rules = rt.duration_rules.clone();
            duration_rules.extend(ctx.registry.duration_rules());
            let duration_secs = extract_duration_secs(ctx.media_info, &duration_rules);
            let bin = resolve_contract_binary(ctx.cache_mgr, rt, ctx.tool_versions);
            let on_p = ctx.on_progress.clone();
            let first_attempt = run_command(
                command_spec_from_contract(rt, &bin, &args, duration_secs, path_env.clone()),
                move |pct, line| on_p(pct, line),
            )
            .await;

            let (outcome, fallback_message) = match first_attempt {
                Ok(out) => (out, None),
                Err(first_err) => {
                    if let Some(rule) = find_retry_fallback(rt, node_config) {
                        tracing::warn!(
                            executable = %rt.executable,
                            error = %first_err,
                            "exécution en échec, tentative de repli contractuel"
                        );
                        let (retry_args, _) =
                            build_binary_argv(rt, node_config, &paths, Some(&rule.overrides))
                                .map_err(|err| BinaryStepError {
                                    code: error_code,
                                    message: format!("{} (repli) : {err}", rt.executable),
                                    excerpt: extract_excerpt(&first_err),
                                })?;
                        let on_p2 = ctx.on_progress.clone();
                        let retry_out = run_command(
                            command_spec_from_contract(
                                rt,
                                &bin,
                                &retry_args,
                                duration_secs,
                                path_env.clone(),
                            ),
                            move |pct, line| on_p2(pct, line),
                        )
                        .await
                        .map_err(|err| BinaryStepError {
                            code: error_code,
                            message: format!("{} (repli) : {err}", rt.executable),
                            excerpt: extract_excerpt(&err),
                        })?;
                        (retry_out, rule.message.clone())
                    } else {
                        return Err(BinaryStepError {
                            code: error_code,
                            message: format!("{} : {first_err}", rt.executable),
                            excerpt: extract_excerpt(&first_err),
                        });
                    }
                }
            };

            post_validate_output(
                ctx.registry,
                ctx.cache_mgr,
                ctx.tool_versions,
                &rt.post_validate_spec(),
                Some(ctx.media_info),
                ctx.working_file,
                &destination,
            )
            .await
            .map_err(|err| BinaryStepError {
                code: error_code,
                message: format!("validation sortie : {err}"),
                excerpt: outcome.excerpt.clone(),
            })?;

            let msg = select_status_message(rt, outcome.warning, fallback_message);
            Ok(BinaryStepOutcome {
                next_working_file: Some(destination),
                updated_media_info: None,
                warning: outcome.warning,
                is_staged_output: true,
                modified_in_place: false,
                message: msg,
                excerpt: outcome.excerpt,
            })
        }
    }
}

fn find_retry_fallback<'a>(
    rt: &'a BinaryRuntime,
    node_config: &Value,
) -> Option<&'a crate::flows::contract::RetryFallbackRule> {
    if rt.retry_fallback.is_empty() {
        return None;
    }
    let mut params = merge_params(&rt.defaults, node_config);
    apply_derived(&mut params, &rt.derived);
    rt.retry_fallback
        .iter()
        .find(|rule| evaluate_condition(&rule.when, &params))
}

fn command_spec_from_contract<'a>(
    rt: &'a BinaryRuntime,
    bin: &'a Path,
    args: &'a [String],
    duration_secs: Option<f64>,
    path_env: Option<std::ffi::OsString>,
) -> CommandSpec<'a> {
    let mut spec = CommandSpec::new(&rt.executable, bin, args);
    spec.path_env = path_env;
    spec.progress_parser = rt.console.progress_format;
    spec.progress_stream = rt.console.progress_stream;
    spec.success_exit_codes = &rt.console.success_exit_codes;
    spec.warning_exit_codes = &rt.console.warning_exit_codes;
    spec.duration_secs = duration_secs;
    spec.timeout = Duration::from_secs(rt.console.timeout_seconds.max(1));
    spec
}

pub(crate) async fn post_validate_output(
    registry: &OperationRegistry,
    cache_mgr: &ToolCacheManager,
    tool_versions: &HashMap<String, String>,
    spec: &PostValidateSpec,
    cached_before: Option<&Value>,
    before_path: &Path,
    after_path: &Path,
) -> anyhow::Result<()> {
    match spec.mode {
        PostValidateMode::None => Ok(()),
        PostValidateMode::NonEmpty => ensure_non_empty_file(after_path).await,
        PostValidateMode::MagicBytes => {
            let rule = spec
                .magic
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("règle magic-bytes manquante dans le contrat"))?;
            validate_magic_bytes(after_path, rule).await
        }
        PostValidateMode::ValidProbe => {
            ensure_non_empty_file(after_path).await?;
            let probe = registry
                .primary_probe_for_file(after_path)
                .ok_or_else(|| anyhow::anyhow!("sonde primaire manquante pour la sortie"))?;
            run_probe_contract(cache_mgr, tool_versions, probe, after_path)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            Ok(())
        }
        PostValidateMode::ProbePreserve => {
            let before = match cached_before {
                Some(existing) => existing.clone(),
                None => {
                    let before_probe =
                        registry
                            .primary_probe_for_file(before_path)
                            .ok_or_else(|| {
                                anyhow::anyhow!("sonde primaire manquante pour la source")
                            })?;
                    run_probe_contract(cache_mgr, tool_versions, before_probe, before_path)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?
                }
            };
            let after_probe = registry
                .primary_probe_for_file(after_path)
                .ok_or_else(|| anyhow::anyhow!("sonde primaire manquante pour la sortie"))?;
            let after = run_probe_contract(cache_mgr, tool_versions, after_probe, after_path)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            validate_probe_preserved(&before, &after, &spec.preserve_paths)
        }
    }
}

fn select_status_message(
    rt: &BinaryRuntime,
    warning: bool,
    override_msg: Option<String>,
) -> Option<String> {
    if override_msg.is_some() {
        return override_msg;
    }
    if warning && rt.warning_message.is_some() {
        return rt.warning_message.clone();
    }
    rt.success_message.clone()
}

fn extract_excerpt(err: &ToolRunError) -> Option<String> {
    match err {
        ToolRunError::Failed { excerpt, .. } => excerpt.clone(),
        _ => None,
    }
}
