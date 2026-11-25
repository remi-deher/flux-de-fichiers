//! Gestionnaires HTTP pour l'API REST des flux de traitement (Flows).

use super::{AppState, ManageFlowsAuth, ViewFlowsAuth};
use crate::{
    domain::UpdateFlowRequest,
    error::{AppError, AppResult},
    plugins, repository,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use forgearr_shared::flows::{FlowGraph, OwnedBlockSpec, legacy_references};
use std::path::PathBuf;

/// Catalogue des blocs pour la palette : issu exclusivement des dépôts de plugins synchronisés.
pub async fn list_blocks(
    State(s): State<AppState>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<Vec<OwnedBlockSpec>>> {
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    Ok(Json(s.catalog_cache.blocks(&roots).await))
}

/// Liste l'ensemble des flux configurés en base de données.
pub async fn list_flows(
    State(s): State<AppState>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<Vec<crate::domain::FlowRecord>>> {
    let mut flows = repository::list_flows(&s.pool).await?;
    for flow in &mut flows {
        flow.definition = crate::flows::mask_definition(&flow.definition);
    }
    Ok(Json(flows))
}

/// Récupère la définition détaillée d'un flux spécifique.
pub async fn get_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<crate::domain::FlowRecord>> {
    let mut flow = repository::get_flow(&s.pool, &id).await?;
    flow.definition = crate::flows::mask_definition(&flow.definition);
    Ok(Json(flow))
}

/// Métadonnées du dernier vrai job du flux, pour l'échantillon de simulation (404 s'il n'y en a pas).
pub async fn media_sample(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<forgearr_shared::flow_analysis::FlowMediaSample>> {
    repository::latest_flow_media_sample(&s.pool, &id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// Versions figées d'un flux, la plus récente d'abord.
pub async fn list_versions(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<Vec<crate::domain::FlowVersionSummary>>> {
    Ok(Json(repository::list_flow_versions(&s.pool, &id).await?))
}

/// Une version figée d'un flux ; les en-têtes secrets sont masqués comme pour le flux courant.
pub async fn get_version(
    State(s): State<AppState>,
    Path((id, version)): Path<(String, i64)>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<crate::domain::FlowRecord>> {
    let mut flow = repository::get_flow_version(&s.pool, &id, version).await?;
    flow.definition = crate::flows::mask_definition(&flow.definition);
    Ok(Json(flow))
}

/// Définition enregistrée d'un flux ; `None` s'il n'existe pas encore.
async fn saved_definition(s: &AppState, id: &str) -> AppResult<Option<String>> {
    match repository::get_flow(&s.pool, id).await {
        Ok(flow) => Ok(Some(flow.definition)),
        Err(AppError::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Définition aux formats actuels : types de blocs renommés, `{{…}}` réécrit en `{$…}` (ADR 0026).
fn upgraded_definition(definition: String) -> String {
    let renamed = plugins::rename_legacy_block_kinds(&definition).unwrap_or(definition);
    legacy_references::upgrade_legacy_definition(&renamed).unwrap_or(renamed)
}

/// Refuse un bloc que rien d'installé ne porte, sauf s'il est déjà dans le flux enregistré.
async fn ensure_known_blocks(
    s: &AppState,
    roots: &[PathBuf],
    registry: &crate::flows::OperationRegistry,
    definition: &str,
    saved: Option<&str>,
) -> AppResult<()> {
    let blocks = s.catalog_cache.blocks(roots).await;
    let unknown = crate::flows::added_unknown_blocks(
        &FlowGraph::parse(definition),
        saved.map(FlowGraph::parse).as_ref(),
        &blocks,
        registry,
    );
    if unknown.is_empty() {
        return Ok(());
    }
    let problems: Vec<String> = unknown
        .iter()
        .map(|kind| crate::flows::unknown_block_message(kind))
        .collect();
    Err(AppError::UnknownBlocks(problems.join(" | ")))
}

/// Enregistre ou met à jour la définition d'un flux (sans installer d'outils à l'enregistrement).
pub async fn save_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManageFlowsAuth,
    Json(payload): Json<UpdateFlowRequest>,
) -> AppResult<(
    StatusCode,
    axum::http::HeaderMap,
    Json<crate::domain::FlowRecord>,
)> {
    if payload.name.trim().is_empty() {
        return Err(AppError::Invalid);
    }
    let condition_errors = crate::flows::definition_condition_errors(&payload.definition);
    if !condition_errors.is_empty() {
        tracing::warn!(flow_id = %id, errors = ?condition_errors, "condition refusée à l'enregistrement du flux");
        return Err(AppError::Invalid);
    }
    super::flow_links::ensure_valid_links(&s.pool, &id, &payload.definition).await?;
    let mut payload = payload;
    payload.definition = upgraded_definition(payload.definition);
    let saved = saved_definition(&s, &id).await?;
    if crate::flows::has_flow_secrets(&payload.definition) {
        let secrets = crate::secrets::SecretBox::open(&s.config.config_dir)?;
        payload.definition =
            crate::flows::seal_definition(&secrets, &id, &payload.definition, saved.as_deref())?;
    }
    // Ce que le flux utilise ne peut être ni repris ni retiré entre le contrôle et l'enregistrement.
    let _installations = plugins::share_installations().await;
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    let registry = s.catalog_cache.operations(&roots).await;
    if let Ok(graph) =
        serde_json::from_str::<forgearr_shared::flows::FlowGraph>(&payload.definition)
    {
        let problems = crate::flows::check_references(&graph, &registry);
        if !problems.is_empty() {
            return Err(AppError::InvalidFlowVariables(problems.join(" | ")));
        }
    }
    ensure_known_blocks(&s, &roots, &registry, &payload.definition, saved.as_deref()).await?;
    let mut flow = repository::save_flow(&s.pool, &id, &payload).await?;
    flow.definition = crate::flows::mask_definition(&flow.definition);

    let mut headers = axum::http::HeaderMap::new();
    if !registry.is_empty() {
        let cache_mgr = s.tool_cache();
        let mut configured = std::collections::HashSet::new();
        for plugin_id in registry.required_settings_plugins_for_flow(Some(&flow)) {
            if let Ok(Some(raw)) = repository::get_plugin_settings(&s.pool, &plugin_id).await
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
                && val.as_object().is_some_and(|obj| !obj.is_empty())
            {
                configured.insert(plugin_id);
            }
        }
        let mut warnings = registry.flow_readiness_warnings(&flow, &cache_mgr, &configured);
        warnings.extend(crate::flows::reference_warnings(
            &forgearr_shared::flows::FlowGraph::parse(&payload.definition),
            &registry,
        ));
        if !warnings.is_empty() {
            tracing::warn!(flow_id = %id, warnings = ?warnings, "dépendances ou réglages manquants lors de l'enregistrement du flux");
            if let Ok(val) = axum::http::HeaderValue::from_str(&warnings.join(" | ")) {
                headers.insert("x-flux-warnings", val);
            }
        }
    }
    Ok((StatusCode::OK, headers, Json(flow)))
}

/// Supprime un flux personnalisé (interdit sur le flux actif par défaut).
pub async fn delete_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManageFlowsAuth,
) -> AppResult<StatusCode> {
    repository::delete_flow(&s.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateFlowRequest {
    pub definition: serde_json::Value,
    /// Flux validé, pour détecter un appel de sous-flux qui reviendrait sur lui (ADR 0026).
    #[serde(default)]
    pub flow_id: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowValidationReport {
    pub valid: bool,
    pub node_count: usize,
    pub connection_count: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

fn has_cycle(graph: &forgearr_shared::flows::FlowGraph) -> bool {
    use std::collections::{HashMap, VecDeque};
    let mut indegree: HashMap<&str, usize> =
        graph.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for conn in &graph.connections {
        if indegree.contains_key(conn.from.as_str()) && indegree.contains_key(conn.to.as_str()) {
            *indegree.entry(conn.to.as_str()).or_insert(0) += 1;
            adj.entry(conn.from.as_str())
                .or_default()
                .push(conn.to.as_str());
        }
    }
    let mut queue: VecDeque<&str> = indegree
        .iter()
        .filter_map(|(&id, &deg)| (deg == 0).then_some(id))
        .collect();
    let mut visited = 0usize;
    while let Some(u) = queue.pop_front() {
        visited += 1;
        if let Some(neighbors) = adj.get(u) {
            for &v in neighbors {
                if let Some(deg) = indegree.get_mut(v) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }
    visited < graph.nodes.len()
}

/// Valide la structure d'un graphe de flux (blocs connus, identifiants uniques, champs requis, absence de cycle).
pub async fn validate_flow(
    State(s): State<AppState>,
    _auth: ViewFlowsAuth,
    Json(payload): Json<ValidateFlowRequest>,
) -> AppResult<Json<FlowValidationReport>> {
    let raw = match payload.definition {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    };
    let graph: forgearr_shared::flows::FlowGraph =
        serde_json::from_str(&raw).map_err(|_| AppError::Invalid)?;
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    let blocks = s.catalog_cache.blocks(&roots).await;
    let registry = s.catalog_cache.operations(&roots).await;

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if graph.nodes.is_empty() {
        errors.push("Le flux ne contient aucun nœud.".to_owned());
    }
    let mut seen_ids = std::collections::HashSet::new();
    for node in &graph.nodes {
        if node.id.trim().is_empty() {
            errors.push("Un nœud possède un identifiant vide.".to_owned());
        } else if !seen_ids.insert(node.id.as_str()) {
            errors.push(format!("Identifiant de nœud dupliqué : {}", node.id));
        }
        if let Some(spec) = forgearr_shared::flows::find_owned(&blocks, &node.kind) {
            for field in &spec.fields {
                if field.required && field.default.is_none() {
                    let missing = node.config.get(&field.name).is_none_or(|v| {
                        v.is_null() || v.as_str().is_some_and(|s| s.trim().is_empty())
                    });
                    if missing {
                        errors.push(format!(
                            "Nœud {} ({}) : champ requis manquant « {} »",
                            node.id, spec.label, field.name
                        ));
                    }
                }
            }
        } else if crate::flows::is_unknown_block(node, &blocks, &registry) {
            errors.push(format!(
                "Nœud {} : {}",
                node.id,
                crate::flows::unknown_block_message(node.kind.as_str())
            ));
        }
    }
    for conn in &graph.connections {
        if !seen_ids.contains(conn.from.as_str()) {
            errors.push(format!("Connexion depuis un nœud inconnu : {}", conn.from));
        }
        if !seen_ids.contains(conn.to.as_str()) {
            errors.push(format!("Connexion vers un nœud inconnu : {}", conn.to));
        }
        if conn.from == conn.to {
            errors.push(format!("Boucle directe sur le nœud : {}", conn.from));
        }
    }
    errors.extend(crate::flows::graph_condition_errors(&graph));
    let flow_id = payload
        .flow_id
        .as_deref()
        .unwrap_or(super::flow_links::UNSAVED_FLOW_ID);
    errors.extend(super::flow_links::definition_link_errors(&s.pool, flow_id, &raw).await?);
    if has_cycle(&graph) {
        errors.push("Le graphe contient au moins un cycle.".to_owned());
    }
    errors.extend(crate::flows::check_references(&graph, &registry));
    warnings.extend(crate::flows::reference_warnings(&graph, &registry));
    Ok(Json(FlowValidationReport {
        valid: errors.is_empty(),
        node_count: graph.nodes.len(),
        connection_count: graph.connections.len(),
        errors,
        warnings,
    }))
}

/// Variables du script d'un nœud : schéma, références disponibles à cet endroit et `config` résolue d'exemple.
pub async fn node_variables(
    State(s): State<AppState>,
    _auth: ViewFlowsAuth,
    Json(payload): Json<forgearr_shared::flows::node_references::NodeVariablesRequest>,
) -> AppResult<Json<forgearr_shared::flows::node_references::NodeVariablesView>> {
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    let registry = s.catalog_cache.operations(&roots).await;
    Ok(Json(crate::flows::node_variables_view(&payload, &registry)))
}

/// Restaure une version figée d'un flux en tant que nouvelle version courante.
pub async fn restore_version(
    State(s): State<AppState>,
    Path((id, version)): Path<(String, i64)>,
    _auth: ManageFlowsAuth,
) -> AppResult<Json<crate::domain::FlowRecord>> {
    let frozen = repository::get_flow_version(&s.pool, &id, version).await?;
    super::flow_links::ensure_valid_links(&s.pool, &id, &frozen.definition).await?;
    let req = UpdateFlowRequest {
        name: frozen.name,
        description: frozen.description,
        file_mask: frozen.file_mask,
        // La version figée reste intacte ; la nouvelle version courante passe aux formats actuels.
        definition: upgraded_definition(frozen.definition),
        is_default: frozen.is_default,
    };
    let saved = saved_definition(&s, &id).await?;
    let _installations = plugins::share_installations().await;
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    let registry = s.catalog_cache.operations(&roots).await;
    ensure_known_blocks(&s, &roots, &registry, &req.definition, saved.as_deref()).await?;
    let mut restored = repository::save_flow(&s.pool, &id, &req).await?;
    restored.definition = crate::flows::mask_definition(&restored.definition);
    Ok(Json(restored))
}
