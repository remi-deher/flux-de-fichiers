//! Présentation métier de l'éditeur : domaines visuels, ports sémantiques, résumés et diagnostics.

use super::condition_edit::GROUP_KEY;
use crate::ui::IconName;
use forgearr_shared::flows::{
    ConditionConfig, ConditionGroup, ConditionOperator, FlowGraph, FlowNode, OwnedBlockSpec,
    OwnedFieldKind, describe_condition, find_owned,
};
use serde_json::Value;

const SUMMARY_MAX_CHARS: usize = 42;
const SUMMARY_MAX_PARTS: usize = 3;
const CHIP_MAX_CHARS: usize = 28;
const CHIP_KEY_MAX_CHARS: usize = 16;
const CHIP_MAX_COUNT: usize = 3;
const MASKED_SECRET: &str = "••••";

/// Puce structurée de paramètre affichée dans le corps d'une carte du canevas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeChip {
    pub key: Option<String>,
    pub value: String,
}

/// Rôle fonctionnel d'un bloc, dictant son accent visuel et son pictogramme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeDomain {
    Input,
    Logic,
    Inspect,
    Process,
    Integration,
    Output,
}

impl NodeDomain {
    /// Déduit le rôle visuel depuis la fiche du bloc (`domain`) ou, à défaut, sa catégorie.
    pub fn from_spec(spec: Option<&OwnedBlockSpec>, category: &str) -> Self {
        match spec.and_then(|s| s.domain.as_deref()) {
            Some("input") => Self::Input,
            Some("logic") => Self::Logic,
            Some("inspect") => Self::Inspect,
            Some("integration") => Self::Integration,
            Some("output") => Self::Output,
            Some(_) => Self::Process,
            None => Self::from_category(category),
        }
    }

    fn from_category(category: &str) -> Self {
        let lower = category.to_lowercase();
        if lower.contains("logique") || lower.contains("logic") {
            Self::Logic
        } else if lower.contains("inspect") {
            Self::Inspect
        } else if lower.contains("entrée") || lower.contains("entree") || lower.contains("input") {
            Self::Input
        } else if lower.contains("sortie") || lower.contains("output") {
            Self::Output
        } else if lower.contains("intégration")
            || lower.contains("integration")
            || lower.contains("notification")
        {
            Self::Integration
        } else {
            Self::Process
        }
    }

    /// Libellé court du domaine métier affiché dans l'en-tête de carte.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Input => "Entrée",
            Self::Logic => "Logique",
            Self::Inspect => "Inspection",
            Self::Process => "Traitement",
            Self::Integration => "Intégration",
            Self::Output => "Sortie",
        }
    }

    /// Jeton CSS portant la couleur d'accent du bloc.
    pub const fn accent_var(self) -> &'static str {
        match self {
            Self::Input => "var(--info)",
            Self::Logic | Self::Output => "var(--success)",
            Self::Inspect => "var(--accent)",
            Self::Process => "var(--primary)",
            Self::Integration => "var(--warning)",
        }
    }

    /// Icône vectorielle associée au bloc.
    pub const fn icon(self) -> IconName {
        match self {
            Self::Input => IconName::Folder,
            Self::Logic => IconName::Sliders,
            Self::Inspect => IconName::Search,
            Self::Process => IconName::Package,
            Self::Integration => IconName::Plugins,
            Self::Output => IconName::Check,
        }
    }

    /// Indice discret affiché lorsqu'aucune puce de configuration n'est présente.
    pub const fn default_hint(self) -> &'static str {
        match self {
            Self::Input => "Point d'entrée du flux",
            Self::Logic => "Branche conditionnelle",
            Self::Inspect => "Analyse des pistes",
            Self::Process => "Paramètres par défaut",
            Self::Integration => "Appel externe",
            Self::Output => "Cible de sortie",
        }
    }
}

/// Tonalité sémantique d'un port de sortie et de ses liaisons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortTone {
    Neutral,
    Success,
    Warning,
}

impl PortTone {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Success => "success",
            Self::Warning => "warning",
        }
    }
}

/// Présentation lisible et colorée d'un port de sortie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortPresentation {
    pub port: String,
    pub label: String,
    pub symbol: &'static str,
    pub tone: PortTone,
}

/// Retourne la présentation sémantique d'un identifiant de port.
pub fn output_presentation(port: &str) -> PortPresentation {
    match port.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "ok" | "matched" => PortPresentation {
            port: port.to_owned(),
            label: "Vrai".to_owned(),
            symbol: "✓",
            tone: PortTone::Success,
        },
        "false" | "no" | "unmatched" => PortPresentation {
            port: port.to_owned(),
            label: "Faux".to_owned(),
            symbol: "✗",
            tone: PortTone::Warning,
        },
        "default" | "out" | "" => PortPresentation {
            port: port.to_owned(),
            label: "Sortie".to_owned(),
            symbol: "→",
            tone: PortTone::Neutral,
        },
        _ => PortPresentation {
            port: port.to_owned(),
            label: port.to_owned(),
            symbol: "→",
            tone: PortTone::Neutral,
        },
    }
}

/// Sorties connues d'un bloc d'après le catalogue plugins et ses liaisons existantes.
pub fn node_output_ports(
    graph: &FlowGraph,
    node: &FlowNode,
    catalog: &[OwnedBlockSpec],
) -> Vec<Option<String>> {
    graph.output_ports(node, catalog)
}

fn is_hidden_field(name: &str, spec: Option<&OwnedBlockSpec>) -> bool {
    if let Some(field) = spec.and_then(|s| s.fields.iter().find(|f| f.name == name))
        && matches!(field.kind, OwnedFieldKind::Headers)
    {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("header")
}

fn truncate_text(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    let prefix: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{prefix}…")
}

fn is_condition_config(
    obj: &serde_json::Map<String, Value>,
    spec: Option<&OwnedBlockSpec>,
) -> bool {
    spec.is_some_and(|s| !s.ports.is_empty())
        || obj.contains_key(GROUP_KEY)
        || (obj.contains_key("property")
            && obj.contains_key("operator")
            && obj.contains_key("target"))
}

fn condition_op_symbol(obj: &serde_json::Map<String, Value>) -> &'static str {
    obj.get("operator")
        .and_then(Value::as_str)
        .map(ConditionOperator::from_loose)
        .unwrap_or_default()
        .symbol()
}

/// Groupe de règles combinées, s'il est présent et non vide.
fn condition_group(obj: &serde_json::Map<String, Value>) -> Option<ConditionGroup> {
    obj.get(GROUP_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value::<ConditionGroup>(value).ok())
        .filter(|group| !group.rules.is_empty())
}

fn condition_parts<'a>(
    obj: &'a serde_json::Map<String, Value>,
    spec: Option<&OwnedBlockSpec>,
) -> Option<(&'a str, &'static str, &'a str)> {
    if !is_condition_config(obj, spec) {
        return None;
    }
    let property = obj.get("property").and_then(Value::as_str).map(str::trim)?;
    let target = obj.get("target").and_then(Value::as_str).map(str::trim)?;
    if property.is_empty() || target.is_empty() {
        return None;
    }
    Some((property, condition_op_symbol(obj), target))
}

fn visible_config_entries<'a>(
    obj: &'a serde_json::Map<String, Value>,
    spec: Option<&'a OwnedBlockSpec>,
    limit: usize,
) -> Vec<(&'a str, &'a str, &'a Value)> {
    let mut entries = Vec::new();
    if let Some(block_spec) = spec {
        for field in &block_spec.fields {
            if entries.len() >= limit {
                break;
            }
            if is_hidden_field(&field.name, Some(block_spec)) {
                continue;
            }
            if let Some(value) = obj.get(&field.name) {
                let label = if field.label.trim().is_empty() {
                    field.name.as_str()
                } else {
                    field.label.as_str()
                };
                if scalar_summary_part(&field.name, value).is_some() {
                    entries.push((field.name.as_str(), label, value));
                }
            }
        }
    } else {
        for (key, value) in obj {
            if entries.len() >= limit {
                break;
            }
            if is_hidden_field(key, None) {
                continue;
            }
            if scalar_summary_part(key, value).is_some() {
                entries.push((key.as_str(), key.as_str(), value));
            }
        }
    }
    entries
}

fn scalar_summary_part(key: &str, value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty() && trimmed != MASKED_SECRET).then(|| trimmed.to_owned())
        }
        Value::Number(num) => Some(num.to_string()),
        Value::Bool(true) => Some(key.to_owned()),
        _ => None,
    }
}

/// Résumé compact de configuration affiché dans la simulation et l'aperçu texte.
pub fn node_summary(node: &FlowNode, spec: Option<&OwnedBlockSpec>) -> Option<String> {
    let obj = node.config.as_object()?;
    if let Some(group) = condition_group(obj) {
        return Some(truncate_text(
            describe_condition(&ConditionConfig::grouped(group)),
            SUMMARY_MAX_CHARS,
        ));
    }
    if let Some((property, op, target)) = condition_parts(obj, spec) {
        return Some(truncate_text(
            format!("{property} {op} {target}"),
            SUMMARY_MAX_CHARS,
        ));
    }
    if is_condition_config(obj, spec) {
        return None;
    }

    let parts: Vec<String> = visible_config_entries(obj, spec, SUMMARY_MAX_PARTS)
        .into_iter()
        .filter_map(|(key, _, value)| scalar_summary_part(key, value))
        .collect();

    (!parts.is_empty()).then(|| truncate_text(parts.join(" · "), SUMMARY_MAX_CHARS))
}

/// Puces structurées de paramètres affichées dans le corps d'une carte sur le canevas.
pub fn node_chips(node: &FlowNode, spec: Option<&OwnedBlockSpec>) -> Vec<NodeChip> {
    let Some(obj) = node.config.as_object() else {
        return Vec::new();
    };
    if let Some(group) = condition_group(obj) {
        return vec![NodeChip {
            key: None,
            value: format!("{} règles ({})", group.rules.len(), group.combinator.label()),
        }];
    }
    if let Some((property, op, target)) = condition_parts(obj, spec) {
        return vec![NodeChip {
            key: Some(truncate_text(property.to_owned(), CHIP_KEY_MAX_CHARS)),
            value: truncate_text(format!("{op} {target}"), CHIP_MAX_CHARS),
        }];
    }
    if is_condition_config(obj, spec) {
        let op = condition_op_symbol(obj);
        return obj
            .get("property")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|prop| !prop.is_empty())
            .map(|prop| {
                vec![NodeChip {
                    key: Some(truncate_text(prop.to_owned(), CHIP_KEY_MAX_CHARS)),
                    value: truncate_text(format!("{op} ?"), CHIP_MAX_CHARS),
                }]
            })
            .unwrap_or_default();
    }

    visible_config_entries(obj, spec, CHIP_MAX_COUNT)
        .into_iter()
        .filter_map(|(key, label, value)| match value {
            Value::Bool(true) => Some(NodeChip {
                key: None,
                value: truncate_text(label.to_owned(), CHIP_MAX_CHARS),
            }),
            _ => scalar_summary_part(key, value).map(|val| NodeChip {
                key: Some(truncate_text(label.to_owned(), CHIP_KEY_MAX_CHARS)),
                value: truncate_text(val, CHIP_MAX_CHARS),
            }),
        })
        .collect()
}

/// Sévérité d'un diagnostic du graphe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

impl DiagnosticLevel {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Alerte ou erreur détectée sur un flux en cours d'édition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowDiagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub node_id: Option<String>,
}

/// Vérifie la cohérence structurelle et la complétude d'un graphe de flux.
pub fn diagnose_graph(graph: &FlowGraph, catalog: &[OwnedBlockSpec]) -> Vec<FlowDiagnostic> {
    let mut diagnostics = Vec::new();
    if graph.nodes.is_empty() {
        diagnostics.push(FlowDiagnostic {
            level: DiagnosticLevel::Warning,
            message: "Le flux ne contient aucun bloc.".to_owned(),
            node_id: None,
        });
        return diagnostics;
    }

    if !catalog.is_empty() {
        let has_entry = graph
            .nodes
            .iter()
            .any(|node| find_owned(catalog, &node.kind).is_some_and(|spec| !spec.has_input));
        if !has_entry {
            diagnostics.push(FlowDiagnostic {
                level: DiagnosticLevel::Warning,
                message: "Aucun bloc d'entrée défini dans le flux.".to_owned(),
                node_id: None,
            });
        }

        let has_terminal = graph
            .nodes
            .iter()
            .any(|node| find_owned(catalog, &node.kind).is_some_and(|spec| spec.terminal));
        if !has_terminal {
            diagnostics.push(FlowDiagnostic {
                level: DiagnosticLevel::Warning,
                message: "Aucun bloc de sortie final défini dans le flux.".to_owned(),
                node_id: None,
            });
        }
    }

    for node in &graph.nodes {
        if let Some(spec) = find_owned(catalog, &node.kind) {
            let missing = spec.missing_fields(&node.config);
            if !missing.is_empty() {
                diagnostics.push(FlowDiagnostic {
                    level: DiagnosticLevel::Error,
                    message: format!(
                        "Le bloc « {} » a des champs obligatoires incomplets ({}).",
                        node.name,
                        missing.join(", ")
                    ),
                    node_id: Some(node.id.clone()),
                });
            }
        }
        if graph.nodes.len() > 1
            && graph.incoming(&node.id).next().is_none()
            && graph.outgoing(&node.id).next().is_none()
        {
            diagnostics.push(FlowDiagnostic {
                level: DiagnosticLevel::Warning,
                message: format!("Le bloc « {} » n'est relié à aucune étape.", node.name),
                node_id: Some(node.id.clone()),
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::flows::{FlowBlockKind, FlowNode};
    use serde_json::{Map, json};

    use super::super::test_support::test_catalog;

    fn make_node(kind: FlowBlockKind, name: &str, config: Value) -> FlowNode {
        FlowNode {
            id: "n1".to_owned(),
            kind,
            name: name.to_owned(),
            config,
            extra: Map::new(),
        }
    }

    #[test]
    fn node_domain_maps_known_kinds_and_unknown_categories() {
        let catalog = test_catalog();
        let entry = find_owned(&catalog, &FlowBlockKind::parse("essentiel.file-input"));
        assert_eq!(NodeDomain::from_spec(entry, "Entrée"), NodeDomain::Input);
        let cond = find_owned(&catalog, &FlowBlockKind::parse("essentiel.condition"));
        assert_eq!(NodeDomain::from_spec(cond, "Logique"), NodeDomain::Logic);
        let video = find_owned(&catalog, &FlowBlockKind::parse("video.encode"));
        assert_eq!(NodeDomain::from_spec(video, "Média"), NodeDomain::Process);
        let audio = find_owned(&catalog, &FlowBlockKind::parse("audio.encode"));
        assert_eq!(NodeDomain::from_spec(audio, "Média"), NodeDomain::Process);
        let mkv = find_owned(&catalog, &FlowBlockKind::parse("video.mkvpropedit"));
        assert_eq!(NodeDomain::from_spec(mkv, "Plugins"), NodeDomain::Process);
        assert_eq!(
            NodeDomain::from_spec(None, "Intégrations"),
            NodeDomain::Integration
        );
        assert_eq!(
            NodeDomain::from_spec(None, "Inspection"),
            NodeDomain::Inspect
        );
        let output = find_owned(&catalog, &FlowBlockKind::parse("essentiel.file-output"));
        assert_eq!(NodeDomain::from_spec(output, "Sortie"), NodeDomain::Output);
    }

    #[test]
    fn output_presentation_formats_semantic_ports() {
        let yes = output_presentation("true");
        assert_eq!(yes.symbol, "✓");
        assert_eq!(yes.label, "Vrai");
        assert_eq!(yes.tone, PortTone::Success);

        let no = output_presentation("false");
        assert_eq!(no.symbol, "✗");
        assert_eq!(no.label, "Faux");
        assert_eq!(no.tone, PortTone::Warning);

        let default = output_presentation("default");
        assert_eq!(default.symbol, "→");
        assert_eq!(default.label, "Sortie");
        assert_eq!(default.tone, PortTone::Neutral);
    }

    #[test]
    fn node_summary_formats_condition_and_ignores_secrets() {
        let catalog = test_catalog();
        let cond = make_node(
            FlowBlockKind::parse("essentiel.condition"),
            "HEVC ?",
            json!({"property": "video.codec", "operator": "eq", "target": "hevc"}),
        );
        assert_eq!(
            node_summary(&cond, find_owned(&catalog, &cond.kind)),
            Some("video.codec ~ hevc".to_owned())
        );
        let grouped = make_node(
            FlowBlockKind::parse("essentiel.condition"),
            "HEVC ou AV1 ?",
            json!({"group": {"combinator": "any", "rules": [
                {"property": "codec", "operator": "exact", "target": "hevc"},
                {"property": "codec", "operator": "exact", "target": "av1"}
            ]}}),
        );
        assert_eq!(
            node_summary(&grouped, find_owned(&catalog, &grouped.kind)),
            Some("(codec = hevc OU codec = av1)".to_owned())
        );
        assert_eq!(
            node_chips(&grouped, find_owned(&catalog, &grouped.kind)),
            vec![NodeChip {
                key: None,
                value: "2 règles (OU)".to_owned(),
            }]
        );

        let hook = make_node(
            FlowBlockKind::parse("essentiel.webhook"),
            "Hook",
            json!({
                "url": "https://hook.test",
                "method": "POST",
                "custom_headers": {"Authorization": "secret"},
                "api_secret": "••••",
                "fail_on_error": true
            }),
        );
        let summary = node_summary(&hook, find_owned(&catalog, &hook.kind));
        assert_eq!(
            summary,
            Some("https://hook.test · POST · fail_on_error".to_owned())
        );

        let long_hook = make_node(
            FlowBlockKind::parse("essentiel.webhook"),
            "Hook long",
            json!({
                "url": "https://example.test/very/long/webhook/endpoint/path",
                "method": "POST",
                "fail_on_error": true
            }),
        );
        let truncated = node_summary(&long_hook, find_owned(&catalog, &long_hook.kind));
        assert!(matches!(
            truncated.as_deref(),
            Some(text) if text.ends_with('…') && text.chars().count() == SUMMARY_MAX_CHARS
        ));

        let extra_secret = make_node(
            FlowBlockKind::parse("custom.step"),
            "Custom",
            json!({"secret_token": "abc", "masked": "••••", "preset": "fast"}),
        );
        assert_eq!(node_summary(&extra_secret, None), Some("fast".to_owned()));
    }

    #[test]
    fn node_chips_extracts_structured_badges_and_masks_secrets() {
        assert_eq!(NodeDomain::Input.label(), "Entrée");
        assert_eq!(NodeDomain::Logic.label(), "Logique");
        assert_eq!(NodeDomain::Inspect.label(), "Inspection");
        assert_eq!(NodeDomain::Process.label(), "Traitement");
        assert_eq!(NodeDomain::Integration.label(), "Intégration");
        assert_eq!(NodeDomain::Output.label(), "Sortie");
        assert_eq!(NodeDomain::Input.default_hint(), "Point d'entrée du flux");

        let catalog = test_catalog();
        let cond_empty_target = make_node(
            FlowBlockKind::parse("essentiel.condition"),
            "Condition",
            json!({"property": "any", "operator": "eq", "target": ""}),
        );
        assert_eq!(
            node_chips(
                &cond_empty_target,
                find_owned(&catalog, &cond_empty_target.kind)
            ),
            vec![NodeChip {
                key: Some("any".to_owned()),
                value: "~ ?".to_owned(),
            }]
        );
        let cond_blank = make_node(
            FlowBlockKind::parse("essentiel.condition"),
            "Condition vide",
            json!({}),
        );
        assert!(node_chips(&cond_blank, find_owned(&catalog, &cond_blank.kind)).is_empty());

        let cond = make_node(
            FlowBlockKind::parse("essentiel.condition"),
            "HEVC ?",
            json!({"property": "video.codec", "operator": "eq", "target": "hevc"}),
        );
        assert_eq!(
            node_chips(&cond, find_owned(&catalog, &cond.kind)),
            vec![NodeChip {
                key: Some("video.codec".to_owned()),
                value: "~ hevc".to_owned(),
            }]
        );

        let hook = make_node(
            FlowBlockKind::parse("essentiel.webhook"),
            "Hook",
            json!({
                "url": "https://hook.test",
                "method": "POST",
                "custom_headers": {"Authorization": "secret"},
                "api_secret": "••••",
                "fail_on_error": true
            }),
        );
        assert_eq!(
            node_chips(&hook, find_owned(&catalog, &hook.kind)),
            vec![
                NodeChip {
                    key: Some("URL".to_owned()),
                    value: "https://hook.test".to_owned(),
                },
                NodeChip {
                    key: Some("Méthode".to_owned()),
                    value: "POST".to_owned(),
                },
                NodeChip {
                    key: None,
                    value: "Échec bloquant".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn diagnose_graph_detects_missing_input_output_fields_and_orphans() {
        let catalog = test_catalog();
        let empty = FlowGraph::default();
        let empty_diags = diagnose_graph(&empty, &catalog);
        assert_eq!(empty_diags.len(), 1);
        assert_eq!(empty_diags[0].level, DiagnosticLevel::Warning);

        let mut graph = FlowGraph::default();
        let cond = graph.append(
            FlowBlockKind::parse("essentiel.condition"),
            "Condition vide",
            json!({}),
            &catalog,
        );
        let orphan = graph.append(
            FlowBlockKind::parse("video.inspect"),
            "Orphelin",
            json!({}),
            &catalog,
        );
        let diags = diagnose_graph(&graph, &catalog);
        assert!(diags.iter().any(|d| d.message.contains("bloc d'entrée")));
        assert!(diags.iter().any(|d| d.message.contains("bloc de sortie")));
        assert!(
            diags.iter().any(|d| d.level == DiagnosticLevel::Error
                && d.node_id.as_deref() == Some(cond.as_str()))
        );
        assert!(diags.iter().any(|d| d.level == DiagnosticLevel::Warning
            && d.node_id.as_deref() == Some(orphan.as_str())));
    }
}
