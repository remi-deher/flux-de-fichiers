//! Références disponibles pour le nœud script d'un flux, contrôle à l'enregistrement et aperçu
//! résolu (ADR 0025, phase D ; modèle unique `{$…}` de l'ADR 0026).
//!
//! L'aperçu passe par la même portée ([`NodeScope`]) et la même fonction
//! ([`resolve_node_config`]) que l'exécution : ce qu'il montre est ce que le script recevrait.

use super::{
    OperationRegistry, OperationRuntime, ScriptRuntime,
    substitution::{set_file_variables, set_location_variables},
    variables::{
        NodeOutput, NodeScope, exposed_value, mask_secrets_for_preview, resolve_node_config,
    },
};
use forgearr_shared::{
    exposed_variables::{ExposedVariable, ExposedVariables},
    flows::{
        FlowGraph, FlowNode, FlowVariables, TEMPLATE_CLOSE, TEMPLATE_OPEN, apply_set_variable,
        is_set_variable_kind,
        legacy_references::{LegacyConversion, upgrade_legacy_references, upgrade_legacy_value},
        node_references::{
            NodeVariablesRequest, NodeVariablesView, ReferenceSource, ReferenceSuggestion,
            flow_variable_suggestions,
        },
        scalar_to_string,
        template_scope::{
            MEDIA_NAMESPACE, NODES_NAMESPACE, NodeOutputField, PATH_SEPARATOR, ScopeNamespace,
            TemplateScope, node_reference, render_in_scope, split_node_reference,
        },
        template_tokens,
    },
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const SAMPLE_WORKING_FILE: &str = "/travail/fichier.mkv";
const SAMPLE_SOURCE_FILE: &str = "/source/fichier.mkv";
const SAMPLE_WORK_DIR: &str = "/travail";
const MAX_SAMPLE_PATHS: usize = 80;
const MAX_SAMPLE_DEPTH: usize = 5;
const MAX_SAMPLE_ARRAY_ITEMS: usize = 3;

/// Script que le nœud exécuterait : celui de la librairie choisi par `scriptId`, sinon celui du bloc.
pub(super) fn effective_script_runtime(
    registry: &OperationRegistry,
    node: &FlowNode,
    base: &ScriptRuntime,
) -> ScriptRuntime {
    let mut effective = base.clone();
    if effective.code.is_none()
        && node.config.get("mode").and_then(Value::as_str) != Some("inline")
        && let Some(target_op) = registry.library_script_operation(node)
        && let OperationRuntime::Script(target_rt) = &target_op.runtime
    {
        let fallback_tool = effective.tool.clone();
        effective = target_rt.clone();
        if effective.tool.is_none() {
            effective.tool = fallback_tool;
        }
    }
    effective
}

fn script_runtime_for_node(registry: &OperationRegistry, node: &FlowNode) -> Option<ScriptRuntime> {
    let op = registry.resolve_node_operation(node)?;
    let OperationRuntime::Script(base) = &op.runtime else {
        return None;
    };
    Some(effective_script_runtime(registry, node, base))
}

/// Identifiants des nœuds qui s'exécutent avant `node_id` (tous les ancêtres), dans l'ordre du graphe.
pub fn upstream_node_ids(graph: &FlowGraph, node_id: &str) -> Vec<String> {
    let mut found: HashSet<&str> = HashSet::new();
    let mut pending = vec![node_id];
    while let Some(current) = pending.pop() {
        for connection in graph.connections.iter().filter(|c| c.to == current) {
            if connection.from != node_id && found.insert(connection.from.as_str()) {
                pending.push(connection.from.as_str());
            }
        }
    }
    graph
        .nodes
        .iter()
        .filter(|node| found.contains(node.id.as_str()))
        .map(|node| node.id.clone())
        .collect()
}

fn collect_leaves(value: &Value, prefix: &str, depth: usize, out: &mut Vec<(String, String)>) {
    if out.len() >= MAX_SAMPLE_PATHS || depth > MAX_SAMPLE_DEPTH {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                collect_leaves(
                    item,
                    &format!("{prefix}{PATH_SEPARATOR}{key}"),
                    depth + 1,
                    out,
                );
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().take(MAX_SAMPLE_ARRAY_ITEMS).enumerate() {
                collect_leaves(
                    item,
                    &format!("{prefix}{PATH_SEPARATOR}{index}"),
                    depth + 1,
                    out,
                );
            }
        }
        Value::Null => {}
        scalar => out.push((prefix.to_owned(), scalar_to_string(Some(scalar)))),
    }
}

/// Expressions `media.…` des valeurs d'un exemple de métadonnées, avec leur valeur.
fn media_info_leaves(media_info: &Value) -> Vec<(String, String)> {
    let mut leaves = Vec::new();
    collect_leaves(media_info, MEDIA_NAMESPACE, 0, &mut leaves);
    leaves
}

/// Ids des opérations dont les nœuds précèdent `node_id`.
fn upstream_operation_ids<'a>(
    graph: &FlowGraph,
    node_id: &str,
    registry: &'a OperationRegistry,
) -> HashSet<&'a str> {
    let upstream = upstream_node_ids(graph, node_id);
    graph
        .nodes
        .iter()
        .filter(|node| upstream.contains(&node.id))
        .filter_map(|node| registry.resolve_node_operation(node))
        .map(|op| op.id.as_str())
        .collect()
}

/// Expositions lues au nœud : celles que désigne son script, puis celles des sondes et des plugins
/// en amont pour les espaces de noms qu'il ne lie pas.
fn suggested_exposures<'a>(
    graph: &FlowGraph,
    node_id: &str,
    registry: &'a OperationRegistry,
) -> Vec<&'a ExposedVariables> {
    let providers = upstream_operation_ids(graph, node_id, registry);
    let requires = graph
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| script_runtime_for_node(registry, node))
        .map(|runtime| runtime.requires)
        .unwrap_or_default();
    let mut exposures = registry.exposures_bound_by(&requires);
    for fallback in registry.exposed_variables_where(|op| {
        OperationRegistry::is_probe(op) || providers.contains(op.id.as_str())
    }) {
        if !exposures
            .iter()
            .any(|known| known.namespace == fallback.namespace)
        {
            exposures.push(fallback);
        }
    }
    exposures
}

/// Expressions utilisables au nœud `node_id` : variables du flux, métadonnées, nœuds en amont et
/// variables exposées par les plugins qui s'exécutent avant lui (sondes comprises). `scope` donne
/// les valeurs d'exemple.
pub fn reference_suggestions(
    graph: &FlowGraph,
    node_id: &str,
    registry: &OperationRegistry,
    sample_media: &Value,
    scope: &dyn TemplateScope,
) -> Vec<ReferenceSuggestion> {
    // Valeur d'exemple connue, sinon celle que donne la portée de l'aperçu.
    let suggestion =
        |reference: String, label: String, source, known: Option<String>| ReferenceSuggestion {
            sample: known.or_else(|| {
                scope
                    .resolve(&reference)
                    .map(|value| scalar_to_string(Some(&value)))
            }),
            reference,
            label,
            source,
        };
    let mut out: Vec<ReferenceSuggestion> = flow_variable_suggestions(graph)
        .into_iter()
        .map(|known| suggestion(known.reference, known.label, known.source, None))
        .collect();
    for (path, sample) in media_info_leaves(sample_media) {
        out.push(suggestion(
            path.clone(),
            path,
            ReferenceSource::MediaInfo,
            Some(sample),
        ));
    }
    let upstream = upstream_node_ids(graph, node_id);
    for node in graph
        .nodes
        .iter()
        .filter(|node| upstream.contains(&node.id))
    {
        for field in NodeOutputField::ALL {
            out.push(suggestion(
                node_reference(&node.id, field),
                format!("{} — {}", node.name, field.label()),
                ReferenceSource::Node,
                None,
            ));
        }
    }
    for exposed in suggested_exposures(graph, node_id, registry) {
        for variable in &exposed.variables {
            let base_label = if variable.label.trim().is_empty() {
                exposed.reference(variable)
            } else {
                variable.label.clone()
            };
            let label = if variable.deprecated.is_some() {
                format!("{base_label} (déprécié)")
            } else {
                base_label
            };
            let sample = exposed_value(sample_media, &variable.path)
                .map(|value| scalar_to_string(Some(value)));
            out.push(suggestion(
                exposed.reference(variable),
                label,
                ReferenceSource::Plugin,
                sample,
            ));
        }
    }
    out
}

fn strings_in(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings_in(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings_in(item, out)),
        _ => {}
    }
}

/// Expressions `{$…}` citées par les variables déclarées d'un nœud, anciennes références converties.
fn declared_expressions(node: &FlowNode, runtime: &ScriptRuntime) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for variable in &runtime.variables {
        let mut texts = Vec::new();
        if let Some(value) = node.config.get(&variable.name) {
            strings_in(value, &mut texts);
        }
        for text in &texts {
            for expr in
                template_tokens(&upgrade_legacy_references(text, LegacyConversion::AllPaths))
            {
                found.push((variable.name.clone(), expr));
            }
        }
    }
    found
}

/// Texte d'une expression tel qu'il s'écrit dans un modèle.
fn token_text(expr: &str) -> String {
    format!("{TEMPLATE_OPEN}{expr}{TEMPLATE_CLOSE}")
}

/// Pourquoi une expression n'est pas valable à cet endroit du flux (`None` si elle l'est).
fn reference_problem(
    expr: &str,
    upstream: &HashSet<&str>,
    plugin_references: &HashSet<String>,
) -> Option<&'static str> {
    match ScopeNamespace::builtin(expr) {
        Some(ScopeNamespace::Intrinsic | ScopeNamespace::Custom | ScopeNamespace::Media) => None,
        Some(ScopeNamespace::Node) => split_node_reference(expr).and_then(|(id, _)| {
            (!upstream.contains(id)).then_some("ce nœud ne s'exécute pas avant celui-ci")
        }),
        Some(ScopeNamespace::Http) => Some("réservée aux blocs HTTP"),
        Some(ScopeNamespace::Plugin) | None
            if expr
                .strip_prefix(NODES_NAMESPACE)
                .is_some_and(|rest| rest.starts_with(PATH_SEPARATOR)) =>
        {
            Some("forme attendue : nodes.<id>.message, nextPort ou file")
        }
        Some(ScopeNamespace::Plugin) | None => {
            (!plugin_references.contains(expr)).then_some("variable inconnue")
        }
    }
}

/// Expressions invalides dans les variables déclarées des nœuds script du graphe ; vide si tout
/// est valable. Seules les clés du schéma sont examinées : le code en ligne et les autres clés ne
/// le sont jamais. Les anciennes références `{{…}}` sont contrôlées après conversion.
pub fn check_references(graph: &FlowGraph, registry: &OperationRegistry) -> Vec<String> {
    let mut problems = Vec::new();
    for node in &graph.nodes {
        let Some(runtime) = script_runtime_for_node(registry, node) else {
            continue;
        };
        let plugin_references: HashSet<String> = registry
            .exposures_for_requirements(&runtime.requires)
            .iter()
            .flat_map(|exposed| {
                exposed
                    .variables
                    .iter()
                    .map(|variable| exposed.reference(variable))
            })
            .collect();
        let upstream_ids = upstream_node_ids(graph, &node.id);
        let upstream: HashSet<&str> = upstream_ids.iter().map(String::as_str).collect();
        for (variable, expr) in declared_expressions(node, &runtime) {
            if let Some(problem) = reference_problem(&expr, &upstream, &plugin_references) {
                problems.push(format!(
                    "Nœud {} : variable « {variable} » : référence {} refusée ({problem})",
                    node.id,
                    token_text(&expr)
                ));
            }
        }
    }
    problems
}

/// Avertissement d'un nœud dont la configuration contient encore l'ancienne syntaxe `{{…}}`.
fn legacy_syntax_warning(node: &FlowNode) -> Option<String> {
    (upgrade_legacy_value(&node.config) != node.config).then(|| {
        format!(
            "Nœud {} : ancienne syntaxe {{{{…}}}} : elle sera réécrite en {TEMPLATE_OPEN}…{TEMPLATE_CLOSE} à l'enregistrement",
            node.id
        )
    })
}

/// Avertissements d'un nœud script : références, dans ses variables déclarées, à des variables
/// exposées dépréciées (celles que son script lit). Sans effet sur l'exécution ; chacun cite le
/// message du plugin et la variable de remplacement quand il en désigne une.
fn node_deprecation_warnings(
    node: &FlowNode,
    runtime: &ScriptRuntime,
    registry: &OperationRegistry,
) -> Vec<String> {
    let exposures = registry.exposures_for_requirements(&runtime.requires);
    let deprecated: HashMap<String, (&ExposedVariables, &ExposedVariable)> = exposures
        .iter()
        .flat_map(|exposed| {
            exposed
                .variables
                .iter()
                .filter(|variable| variable.deprecated.is_some())
                .map(move |variable| (exposed.reference(variable), (*exposed, variable)))
        })
        .collect();
    let mut warnings: Vec<String> = Vec::new();
    if deprecated.is_empty() {
        return warnings;
    }
    for (variable, expr) in declared_expressions(node, runtime) {
        let Some((exposed, exposure)) = deprecated.get(&expr) else {
            continue;
        };
        let message = exposure.deprecated.as_deref().unwrap_or_default();
        let replacement = exposed
            .replacement_reference(exposure)
            .map(|target| format!(" ; utilisez {}", token_text(&target)))
            .unwrap_or_default();
        let warning = format!(
            "Nœud {} : variable « {variable} » : la référence {} est dépréciée ({message}){replacement}",
            node.id,
            token_text(&expr)
        );
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
    warnings
}

/// Avertissements d'un nœud : ancienne syntaxe, puis références dépréciées de son script.
fn node_warnings(node: &FlowNode, registry: &OperationRegistry) -> Vec<String> {
    let mut warnings: Vec<String> = legacy_syntax_warning(node).into_iter().collect();
    if let Some(runtime) = script_runtime_for_node(registry, node) {
        warnings.extend(node_deprecation_warnings(node, &runtime, registry));
    }
    warnings
}

/// Avertissements de tout le graphe : ancienne syntaxe et références à des variables exposées
/// dépréciées.
pub fn reference_warnings(graph: &FlowGraph, registry: &OperationRegistry) -> Vec<String> {
    graph
        .nodes
        .iter()
        .flat_map(|node| node_warnings(node, registry))
        .collect()
}

/// Variables de flux d'exemple au nœud : fichier, emplacements, puis `vars.*` écrites par les
/// blocs « Définir une variable » en amont, appliqués dans l'ordre comme le moteur.
fn sample_variables(graph: &FlowGraph, upstream: &[String]) -> FlowVariables {
    let mut variables = FlowVariables::default();
    set_location_variables(
        &mut variables,
        Path::new(SAMPLE_SOURCE_FILE),
        Path::new(SAMPLE_WORK_DIR),
    );
    set_file_variables(&mut variables, Path::new(SAMPLE_WORKING_FILE));
    for node in graph
        .nodes
        .iter()
        .filter(|node| upstream.contains(&node.id) && is_set_variable_kind(node.kind.as_str()))
    {
        let config = render_in_scope(&upgrade_legacy_value(&node.config), &variables);
        // Un bloc mal configuré est signalé par la validation ; l'aperçu l'ignore.
        apply_set_variable(&config, &mut variables).ok();
    }
    variables
}

/// Schéma du script du nœud, références disponibles et `config` résolue avec des valeurs d'exemple.
pub fn node_variables_view(
    request: &NodeVariablesRequest,
    registry: &OperationRegistry,
) -> NodeVariablesView {
    let graph = &request.graph;
    let Some(node) = graph.nodes.iter().find(|node| node.id == request.node_id) else {
        return NodeVariablesView {
            error: Some("nœud introuvable dans le flux".to_owned()),
            ..NodeVariablesView::default()
        };
    };
    let Some(runtime) = script_runtime_for_node(registry, node) else {
        return NodeVariablesView::default();
    };
    let sample_media = if request.media_info.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        request.media_info.clone()
    };
    let upstream = upstream_node_ids(graph, &node.id);
    let outputs: HashMap<String, NodeOutput> = upstream
        .iter()
        .map(|id| {
            (
                id.clone(),
                NodeOutput {
                    message: Some(format!("‹message de {id}›")),
                    next_port: Some(format!("‹port de {id}›")),
                    working_file: format!("‹fichier de {id}›"),
                },
            )
        })
        .collect();
    let variables = sample_variables(graph, &upstream);
    let exposures = registry.exposures_for_requirements(&runtime.requires);
    let scope = NodeScope::new(&variables, &sample_media, &outputs, &exposures);
    let preview_config = mask_secrets_for_preview(&runtime.variables, &node.config);
    let (resolved, error) = match resolve_node_config(&runtime.variables, &preview_config, &scope) {
        Ok(config) => (Some(config), None),
        Err(err) => (None, Some(err.to_string())),
    };
    NodeVariablesView {
        references: reference_suggestions(graph, &node.id, registry, &sample_media, &scope),
        warnings: node_warnings(node, registry),
        variables: runtime.variables,
        resolved,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> Result<OperationRegistry, super::super::ContractError> {
        let mut reg = OperationRegistry::new();
        reg.register_json(json!({
            "schemaVersion": "1.1.0", "id": "operation.script.enc", "version": "1.0.0", "name": "Enc",
            "providerId": "script.enc", "providerKind": "script", "capability": "script", "status": "validated",
            "runtime": {"kind": "script", "interpreter": "sh", "code": "true", "variables": [
                {"name": "crf", "type": "number", "default": 23},
                {"name": "cible", "type": "text"},
                {"name": "codec", "type": "text"}
            ]}
        }))?;
        reg.register_json(json!({
            "schemaVersion": "1.1.0", "id": "operation.mi.run", "version": "1.0.0", "name": "MI",
            "providerId": "app.mediainfo", "providerKind": "plugin", "capability": "inspect", "status": "validated",
            "runtime": {"kind": "primitive", "primitive": "entry"},
            "exposes": {"namespace": "mediainfo", "contractVersion": "1.0.0",
                        "variables": [{"name": "video.codec", "label": "Codec vidéo", "path": "video.codec"}]}
        }))?;
        reg.register_block_mapping("block.script.enc", "operation.script.enc");
        reg.register_block_mapping("block.mi", "operation.mi.run");
        Ok(reg)
    }

    fn graph(enc_config: Value) -> FlowGraph {
        serde_json::from_value(json!({
            "nodes": [
                {"id": "in", "type": "file-input", "name": "Entrée"},
                {"id": "mi", "type": "block.mi", "name": "MediaInfo"},
                {"id": "enc", "type": "block.script.enc", "name": "Encodeur", "config": enc_config},
                {"id": "apres", "type": "file-output", "name": "Sortie"}
            ],
            "connections": [
                {"from": "in", "to": "mi"}, {"from": "mi", "to": "enc"}, {"from": "enc", "to": "apres"}
            ]
        }))
        .unwrap_or_default()
    }

    #[test]
    fn upstream_nodes_are_all_ancestors_in_graph_order_and_exclude_the_node_itself() {
        let g = graph(json!({}));
        assert_eq!(upstream_node_ids(&g, "enc"), ["in", "mi"]);
        assert_eq!(upstream_node_ids(&g, "in"), Vec::<String>::new());
        let mut cyclic = g.clone();
        cyclic.connections.push(
            serde_json::from_value(json!({"from": "enc", "to": "in"}))
                .unwrap_or_else(|_| g.connections[0].clone()),
        );
        assert!(!upstream_node_ids(&cyclic, "enc").contains(&"enc".to_owned()));
    }

    #[test]
    fn suggestions_cover_flow_values_metadata_previous_nodes_and_upstream_plugins()
    -> anyhow::Result<()> {
        let reg = registry()?;
        let g = graph(json!({}));
        let media = json!({"video": {"codec": "hevc"}, "streams": [{"lang": "fr"}]});
        let mut variables = FlowVariables::default();
        set_location_variables(&mut variables, Path::new("/in/a.mkv"), Path::new("/w"));
        let refs = reference_suggestions(&g, "enc", &reg, &media, &variables);
        let find = |reference: &str| refs.iter().find(|r| r.reference == reference);
        assert_eq!(
            find("file.path").map(|r| r.source),
            Some(ReferenceSource::Flow)
        );
        assert_eq!(
            find("work.dir").and_then(|r| r.sample.clone()),
            Some("/w".into())
        );
        assert_eq!(
            find("job.id").map(|r| r.source),
            Some(ReferenceSource::Variables)
        );
        assert_eq!(
            find("media.video.codec").and_then(|r| r.sample.clone()),
            Some("hevc".into())
        );
        assert!(find("media.streams.0.lang").is_some());
        assert!(find("nodes.mi.file").is_some());
        assert_eq!(
            find("nodes.mi.message").map(|r| r.source),
            Some(ReferenceSource::Node)
        );
        assert!(find("nodes.apres.message").is_none() && find("nodes.enc.message").is_none());
        let plugin = find("mediainfo.video.codec");
        assert_eq!(plugin.map(|r| r.label.as_str()), Some("Codec vidéo"));
        assert_eq!(plugin.and_then(|r| r.sample.clone()), Some("hevc".into()));
        // Avant le nœud MediaInfo, le plugin n'est pas encore disponible.
        let before = reference_suggestions(&g, "mi", &reg, &media, &variables);
        assert!(before.iter().all(|r| r.source != ReferenceSource::Plugin));
        Ok(())
    }

    #[test]
    fn sampled_metadata_paths_are_bounded() {
        let wide: serde_json::Map<String, Value> =
            (0..200).map(|n| (format!("k{n:03}"), json!(n))).collect();
        assert_eq!(
            media_info_leaves(&Value::Object(wide)).len(),
            MAX_SAMPLE_PATHS
        );
        let array = json!({"a": [1, 2, 3, 4, 5]});
        assert_eq!(media_info_leaves(&array).len(), MAX_SAMPLE_ARRAY_ITEMS);
        let deep = json!({"a": {"b": {"c": {"d": {"e": {"f": {"g": 1}}}}}}});
        assert!(media_info_leaves(&deep).is_empty());
    }

    #[test]
    fn valid_references_pass_and_each_invalid_kind_is_reported() -> anyhow::Result<()> {
        let reg = registry()?;
        let legacy = graph(json!({
            "cible": "{{workingFile}} {{mediaInfo.video.codec}} {{nodes.mi.workingFile}}",
            "codec": "{{mediainfo.video.codec}}"
        }));
        assert!(check_references(&legacy, &reg).is_empty());
        let current = graph(json!({
            "cible": "{$file.path} {$media.video.codec} {$nodes.mi.file} {$vars.x|d}",
            "codec": "{$mediainfo.video.codec}"
        }));
        assert!(check_references(&current, &reg).is_empty());

        let bad = graph(json!({
            "cible": "{$file.path.x} {$nodes.apres.message} {$nodes.mi} {$inconnue.var}",
            "codec": "{$nodes.mi.autre} {$setting.url}"
        }));
        let problems = check_references(&bad, &reg);
        assert_eq!(problems.len(), 6, "{problems:?}");
        assert!(problems.iter().all(|p| p.contains("Nœud enc")));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("{$nodes.apres.message}")
                    && p.contains("ne s'exécute pas avant"))
        );
        assert!(problems.iter().any(|p| p.contains("forme attendue")));
        assert!(problems.iter().any(|p| p.contains("variable inconnue")));
        assert!(problems.iter().any(|p| p.contains("blocs HTTP")));
        Ok(())
    }

    #[test]
    fn only_declared_variables_are_checked() -> anyhow::Result<()> {
        let reg = registry()?;
        let g = graph(json!({"code": "echo {$n'importe.quoi}", "autre": "{$inconnue.var}"}));
        assert!(check_references(&g, &reg).is_empty());
        Ok(())
    }

    #[test]
    fn the_preview_resolves_defaults_and_example_values_or_explains_the_failure()
    -> anyhow::Result<()> {
        let reg = registry()?;
        let request = |config: Value| NodeVariablesRequest {
            graph: graph(config),
            node_id: "enc".into(),
            media_info: json!({"video": {"codec": "hevc"}}),
        };
        let view = node_variables_view(
            &request(json!({"cible": "{$file.path}", "codec": "{$mediainfo.video.codec}"})),
            &reg,
        );
        assert_eq!(view.error, None);
        let resolved = view
            .resolved
            .ok_or_else(|| anyhow::anyhow!("config non résolue"))?;
        assert_eq!(resolved["crf"], json!(23));
        assert_eq!(resolved["cible"], json!("/travail/fichier.mkv"));
        assert_eq!(resolved["codec"], json!("hevc"));
        assert_eq!(view.variables.len(), 3);
        assert!(!view.references.is_empty());

        let failing = node_variables_view(&request(json!({"cible": "{$media.absent}"})), &reg);
        assert!(failing.resolved.is_none());
        assert!(failing.error.is_some_and(|e| e.contains("cible")));

        let missing = NodeVariablesRequest {
            node_id: "inconnu".into(),
            ..request(json!({}))
        };
        assert!(node_variables_view(&missing, &reg).error.is_some());
        let not_a_script = NodeVariablesRequest {
            node_id: "mi".into(),
            ..request(json!({}))
        };
        assert_eq!(
            node_variables_view(&not_a_script, &reg),
            NodeVariablesView::default()
        );
        Ok(())
    }

    fn provider_with_path(op_id: &str, provider: &str, path: &str) -> Value {
        json!({
            "schemaVersion": "1.1.0", "id": op_id, "version": "1.0.0", "name": "Fournisseur",
            "providerId": provider, "providerKind": "plugin", "capability": "probe", "status": "validated",
            "runtime": {"kind": "primitive", "primitive": "entry"},
            "exposes": {"namespace": "mediainfo", "contractVersion": "1.0.0",
                        "variables": [{"name": "video.codec", "path": path}]}
        })
    }

    fn codec_script(op_id: &str, requires: Value) -> Value {
        json!({
            "schemaVersion": "1.1.0", "id": op_id, "version": "1.0.0", "name": "Script",
            "providerId": "script.x", "providerKind": "script", "capability": "script", "status": "validated",
            "runtime": {"kind": "script", "interpreter": "sh", "code": "true",
                        "variables": [{"name": "codec", "type": "text"}], "requires": requires}
        })
    }

    #[test]
    fn two_plugins_sharing_a_namespace_are_told_apart_by_the_script_dependency()
    -> anyhow::Result<()> {
        let mut reg = OperationRegistry::new();
        reg.register_json(provider_with_path("operation.a.probe", "alpha", "a.codec"))?;
        reg.register_json(provider_with_path("operation.b.probe", "beta", "b.codec"))?;
        reg.register_json(codec_script(
            "operation.script.dep",
            json!([{"variable": "mediainfo.video.codec", "provider": "beta"}]),
        ))?;
        reg.register_json(codec_script("operation.script.free", json!([])))?;
        reg.register_block_mapping("block.script.dep", "operation.script.dep");
        reg.register_block_mapping("block.script.free", "operation.script.free");
        let graph: FlowGraph = serde_json::from_value(json!({
            "nodes": [
                {"id": "dep", "type": "block.script.dep", "name": "Dépendant",
                 "config": {"codec": "{{mediainfo.video.codec}}"}},
                {"id": "free", "type": "block.script.free", "name": "Libre",
                 "config": {"codec": "{{mediainfo.video.codec}}"}}
            ],
            "connections": []
        }))?;
        let media = json!({"a": {"codec": "VIDEO-A"}, "b": {"codec": "VIDEO-B"}});
        let preview = |node_id: &str| {
            node_variables_view(
                &NodeVariablesRequest {
                    graph: graph.clone(),
                    node_id: node_id.into(),
                    media_info: media.clone(),
                },
                &reg,
            )
        };

        let bound = preview("dep");
        assert_eq!(bound.error, None);
        assert_eq!(
            bound.resolved.map(|config| config["codec"].clone()),
            Some(json!("VIDEO-B"))
        );
        let sample = bound
            .references
            .iter()
            .find(|reference| reference.reference == "mediainfo.video.codec")
            .and_then(|reference| reference.sample.clone());
        assert_eq!(
            sample.as_deref(),
            Some("VIDEO-B"),
            "le sélecteur propose l'exposition liée"
        );

        let unbound = preview("free");
        assert_eq!(
            unbound.resolved.map(|config| config["codec"].clone()),
            Some(json!("VIDEO-A"))
        );
        assert!(check_references(&graph, &reg).is_empty());
        Ok(())
    }

    fn deprecating_provider() -> Value {
        json!({
            "schemaVersion": "1.1.0", "id": "operation.p.probe", "version": "1.0.0", "name": "Fournisseur",
            "providerId": "plug", "providerKind": "plugin", "capability": "probe", "status": "validated",
            "runtime": {"kind": "primitive", "primitive": "entry"},
            "exposes": {"namespace": "mediainfo", "contractVersion": "1.1.0", "variables": [
                {"name": "video.codec", "label": "Codec", "path": "p.old",
                 "deprecated": "Renommée en video.codec_name.", "replacedBy": "video.codec_name"},
                {"name": "video.codec_name", "label": "Nom du codec", "path": "p.new"},
                {"name": "video.height", "path": "p.h", "deprecated": "Abandonnée."}
            ]}
        })
    }

    #[test]
    fn a_reference_to_a_deprecated_exposed_variable_warns_with_its_replacement()
    -> anyhow::Result<()> {
        let mut reg = OperationRegistry::new();
        reg.register_json(deprecating_provider())?;
        // Le script désigne le fournisseur : le sélecteur ne propose que les plugins disponibles.
        reg.register_json(codec_script(
            "operation.script.dep",
            json!([{"variable": "mediainfo.video.codec", "provider": "plug"}]),
        ))?;
        reg.register_block_mapping("block.script.dep", "operation.script.dep");
        let graph: FlowGraph = serde_json::from_value(json!({
            "nodes": [
                {"id": "ancien", "type": "block.script.dep", "name": "Ancien",
                 "config": {"codec": "{$mediainfo.video.codec} {$mediainfo.video.codec}"}},
                {"id": "sans", "type": "block.script.dep", "name": "Sans remplaçante",
                 "config": {"codec": "{$mediainfo.video.height}"}},
                {"id": "neuf", "type": "block.script.dep", "name": "Récent",
                 "config": {"codec": "{$mediainfo.video.codec_name}"}}
            ],
            "connections": []
        }))?;

        let warnings = reference_warnings(&graph, &reg);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[0].contains("Nœud ancien")
                && warnings[0].contains("{$mediainfo.video.codec}")
                && warnings[0].contains("Renommée en video.codec_name.")
                && warnings[0].contains("utilisez {$mediainfo.video.codec_name}")
        );
        assert!(
            warnings[1].contains("Nœud sans")
                && warnings[1].contains("Abandonnée.")
                && !warnings[1].contains("utilisez")
        );
        assert!(
            check_references(&graph, &reg).is_empty(),
            "une référence dépréciée reste valable"
        );

        let view = node_variables_view(
            &NodeVariablesRequest {
                graph: graph.clone(),
                node_id: "ancien".into(),
                media_info: json!({"p": {"old": "X", "new": "Y"}}),
            },
            &reg,
        );
        assert_eq!(view.error, None);
        assert_eq!(
            view.resolved.map(|config| config["codec"].clone()),
            Some(json!("X X"))
        );
        assert_eq!(view.warnings.len(), 1, "doublon fusionné");
        let label = view
            .references
            .iter()
            .find(|reference| reference.reference == "mediainfo.video.codec")
            .map(|reference| reference.label.clone());
        assert_eq!(label.as_deref(), Some("Codec (déprécié)"));
        let current = node_variables_view(
            &NodeVariablesRequest {
                graph,
                node_id: "neuf".into(),
                media_info: json!({}),
            },
            &reg,
        );
        assert!(current.warnings.is_empty());
        Ok(())
    }

    #[test]
    fn legacy_syntax_is_reported_as_a_warning() -> anyhow::Result<()> {
        let reg = registry()?;
        let legacy = graph(json!({"cible": "{{workingFile}}"}));
        let warnings = reference_warnings(&legacy, &reg);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("Nœud enc") && warnings[0].contains("ancienne syntaxe"));
        assert!(reference_warnings(&graph(json!({"cible": "{$file.path}"})), &reg).is_empty());
        Ok(())
    }

    #[test]
    fn the_preview_of_a_legacy_graph_matches_the_execution_path() -> anyhow::Result<()> {
        let reg = registry()?;
        let legacy = json!({"cible": "{{workDir}}/{{nodes.mi.message}}", "codec": "{{mediaInfo.video.codec}}"});
        let request = NodeVariablesRequest {
            graph: graph(legacy.clone()),
            node_id: "enc".into(),
            media_info: json!({"video": {"codec": "hevc"}}),
        };
        let view = node_variables_view(&request, &reg);
        let mut variables = FlowVariables::default();
        set_location_variables(
            &mut variables,
            Path::new(SAMPLE_SOURCE_FILE),
            Path::new(SAMPLE_WORK_DIR),
        );
        set_file_variables(&mut variables, Path::new(SAMPLE_WORKING_FILE));
        let outputs = HashMap::from([(
            "mi".to_owned(),
            NodeOutput {
                message: Some("‹message de mi›".into()),
                ..NodeOutput::default()
            },
        )]);
        let runtime = graph(legacy.clone())
            .nodes
            .iter()
            .find(|node| node.id == "enc")
            .and_then(|node| script_runtime_for_node(&reg, node))
            .ok_or_else(|| anyhow::anyhow!("script introuvable"))?;
        let exposures = reg.exposures_for_requirements(&runtime.requires);
        let executed = resolve_node_config(
            &runtime.variables,
            &legacy,
            &NodeScope::new(&variables, &request.media_info, &outputs, &exposures),
        )?;
        assert_eq!(view.resolved, Some(executed.clone()));
        assert_eq!(executed["cible"], json!("/travail/‹message de mi›"));
        assert_eq!(executed["codec"], json!("hevc"));
        Ok(())
    }

    #[test]
    fn the_preview_sees_variables_set_upstream() -> anyhow::Result<()> {
        let reg = registry()?;
        let mut g = graph(json!({"cible": "{$vars.suffixe}"}));
        g.nodes.push(serde_json::from_value(json!({
            "id": "set", "type": "set-variable", "name": "Suffixe",
            "config": {"name": "suffixe", "value": "{$file.stem}-x"}
        }))?);
        g.connections
            .push(serde_json::from_value(json!({"from": "mi", "to": "set"}))?);
        g.connections
            .push(serde_json::from_value(json!({"from": "set", "to": "enc"}))?);
        let view = node_variables_view(
            &NodeVariablesRequest {
                graph: g,
                node_id: "enc".into(),
                media_info: Value::Null,
            },
            &reg,
        );
        assert_eq!(
            view.resolved.map(|config| config["cible"].clone()),
            Some(json!("fichier-x"))
        );
        Ok(())
    }
}
