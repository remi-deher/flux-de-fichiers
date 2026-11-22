//! Conversion des anciennes références `{{chemin}}` (ADR 0025) vers les modèles `{$expr}` (ADR 0026).
//!
//! Les définitions déjà stockées (versions de flux immuables, configurations de nœuds, manifestes
//! de plugins) restent exécutables : le moteur, l'aperçu et le simulateur les convertissent à la
//! lecture, et l'enregistrement d'un flux les réécrit au nouveau format. C'est la seule fonction
//! qui connaît l'ancienne syntaxe.

use super::{
    FlowGraph,
    template_scope::{MEDIA_NAMESPACE, NODES_NAMESPACE, NodeOutputField, PATH_SEPARATOR},
    variables::{
        IntrinsicVariable, RAW_CONFIG_KEYS, TEMPLATE_CLOSE, TEMPLATE_OPEN, map_strings_infallible,
    },
};
use serde_json::Value;
use std::borrow::Cow;

const LEGACY_OPEN: &str = "{{";
const LEGACY_CLOSE: &str = "}}";
/// Champ « fichier courant » d'un nœud dans l'ancienne syntaxe (`nodes.<id>.workingFile`).
const LEGACY_NODE_FILE_FIELD: &str = "workingFile";

/// Racines de l'ancienne syntaxe dont le nom change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegacyRoot {
    WorkingFile,
    SourceFile,
    WorkDir,
    MediaInfo,
}

impl LegacyRoot {
    const ALL: [Self; 4] = [
        Self::WorkingFile,
        Self::SourceFile,
        Self::WorkDir,
        Self::MediaInfo,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::WorkingFile => "workingFile",
            Self::SourceFile => "sourceFile",
            Self::WorkDir => "workDir",
            Self::MediaInfo => "mediaInfo",
        }
    }

    /// Racine équivalente dans les modèles `{$…}`.
    const fn target(self) -> &'static str {
        match self {
            Self::WorkingFile => IntrinsicVariable::FilePath.key(),
            Self::SourceFile => IntrinsicVariable::SourcePath.key(),
            Self::WorkDir => IntrinsicVariable::WorkDir.key(),
            Self::MediaInfo => MEDIA_NAMESPACE,
        }
    }
}

/// Même règle que l'ancien analyseur : un chemin non vide de `[A-Za-z0-9._-]`.
fn is_legacy_path(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Étendue de la conversion des anciennes références.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyConversion {
    /// Seulement les racines du moteur (`workingFile`, `sourceFile`, `workDir`, `mediaInfo.*`,
    /// `nodes.<id>.<champ>`) : tout autre `{{…}}` d'une configuration (gabarit destiné à l'outil
    /// appelé, moustache d'un corps JSON) reste littéral.
    KnownRoots,
    /// Tout chemin valide, y compris les variables exposées par les plugins : réservé aux valeurs
    /// des variables déclarées d'un script, seules interprétées par l'ancien moteur.
    AllPaths,
}

/// Vrai si l'ancien chemin désigne une racine du moteur.
fn is_known_legacy_root(path: &str) -> bool {
    let is_root = |root: &str| {
        path == root
            || path
                .strip_prefix(root)
                .is_some_and(|rest| rest.starts_with(PATH_SEPARATOR))
    };
    LegacyRoot::ALL.into_iter().any(|root| is_root(root.name()))
        || path
            .strip_prefix(NODES_NAMESPACE)
            .and_then(|rest| rest.strip_prefix(PATH_SEPARATOR))
            .and_then(|rest| rest.rsplit_once(PATH_SEPARATOR))
            .is_some_and(|(node_id, field)| {
                !node_id.is_empty()
                    && (field == LEGACY_NODE_FILE_FIELD
                        || NodeOutputField::from_key(field).is_some())
            })
}

impl LegacyConversion {
    fn converts(self, path: &str) -> bool {
        self == Self::AllPaths || is_known_legacy_root(path)
    }
}

/// Expression `{$…}` d'un ancien chemin ; les variables exposées par les plugins gardent leur nom.
pub fn upgrade_legacy_reference(path: &str) -> String {
    for root in LegacyRoot::ALL {
        if path == root.name() {
            return root.target().to_owned();
        }
        if let Some(rest) = path
            .strip_prefix(root.name())
            .and_then(|rest| rest.strip_prefix(PATH_SEPARATOR))
        {
            return format!("{}{PATH_SEPARATOR}{rest}", root.target());
        }
    }
    if let Some(node_id) = path
        .strip_prefix(NODES_NAMESPACE)
        .and_then(|rest| rest.strip_prefix(PATH_SEPARATOR))
        .and_then(|rest| rest.strip_suffix(LEGACY_NODE_FILE_FIELD))
        .and_then(|rest| rest.strip_suffix(PATH_SEPARATOR))
        .filter(|id| !id.is_empty())
    {
        return super::template_scope::node_reference(node_id, NodeOutputField::File);
    }
    path.to_owned()
}

/// Réécrit chaque `{{chemin}}` valide couvert par `conversion` en `{$cible}` ; le reste (dont
/// l'échappement `{{$`) est conservé. Sans ancienne référence, le texte est rendu tel quel, sans copie.
pub fn upgrade_legacy_references(text: &str, conversion: LegacyConversion) -> Cow<'_, str> {
    if !text.contains(LEGACY_OPEN) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut literal_from = 0;
    let mut cursor = 0;
    let mut changed = false;
    while let Some(open_offset) = text[cursor..].find(LEGACY_OPEN) {
        let open = cursor + open_offset;
        let inner_start = open + LEGACY_OPEN.len();
        let Some(close_offset) = text[inner_start..].find(LEGACY_CLOSE) else {
            break;
        };
        let close = inner_start + close_offset;
        let path = text[inner_start..close].trim();
        if is_legacy_path(path) && conversion.converts(path) {
            out.push_str(&text[literal_from..open]);
            out.push_str(TEMPLATE_OPEN);
            out.push_str(&upgrade_legacy_reference(path));
            out.push(TEMPLATE_CLOSE);
            literal_from = close + LEGACY_CLOSE.len();
            cursor = literal_from;
            changed = true;
        } else {
            cursor = inner_start;
        }
    }
    if !changed {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[literal_from..]);
    Cow::Owned(out)
}

/// Convertit toutes les chaînes d'une valeur JSON.
pub fn upgrade_legacy_strings(value: &Value, conversion: LegacyConversion) -> Value {
    map_strings_infallible(value, &mut |text| {
        Value::String(upgrade_legacy_references(text, conversion).into_owned())
    })
}

/// Convertit la configuration d'un nœud (racines du moteur seulement, voir
/// [`LegacyConversion::KnownRoots`]) ; le code et les secrets (`RAW_CONFIG_KEYS`) restent intacts.
pub fn upgrade_legacy_value(config: &Value) -> Value {
    match config {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let upgraded = if RAW_CONFIG_KEYS.contains(&key.as_str()) {
                        value.clone()
                    } else {
                        upgrade_legacy_strings(value, LegacyConversion::KnownRoots)
                    };
                    (key.clone(), upgraded)
                })
                .collect(),
        ),
        other => upgrade_legacy_strings(other, LegacyConversion::KnownRoots),
    }
}

/// Convertit les configurations des nœuds d'un graphe ; `true` si au moins une a changé.
pub fn upgrade_legacy_graph(graph: &mut FlowGraph) -> bool {
    let mut changed = false;
    for node in &mut graph.nodes {
        let upgraded = upgrade_legacy_value(&node.config);
        if upgraded != node.config {
            node.config = upgraded;
            changed = true;
        }
    }
    changed
}

/// Définition de flux (JSON) réécrite au nouveau format ; `None` si elle n'est pas un graphe ou
/// si elle est déjà à jour. Sert à l'enregistrement : les versions figées ne sont jamais réécrites.
pub fn upgrade_legacy_definition_value(definition: &Value) -> Option<Value> {
    let mut graph: FlowGraph = serde_json::from_value(definition.clone()).ok()?;
    if !upgrade_legacy_graph(&mut graph) {
        return None;
    }
    serde_json::to_value(&graph).ok()
}

/// Variante texte de [`upgrade_legacy_definition_value`] pour une définition stockée.
pub fn upgrade_legacy_definition(definition: &str) -> Option<String> {
    let value: Value = serde_json::from_str(definition).ok()?;
    upgrade_legacy_definition_value(&value).map(|upgraded| upgraded.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_legacy_root_has_its_target() {
        let cases = [
            ("workingFile", "file.path"),
            ("sourceFile", "source.path"),
            ("workDir", "work.dir"),
            ("mediaInfo.video.codec", "media.video.codec"),
            ("mediaInfo.streams.0.lang", "media.streams.0.lang"),
            ("nodes.probe.message", "nodes.probe.message"),
            ("nodes.probe.nextPort", "nodes.probe.nextPort"),
            ("nodes.probe.workingFile", "nodes.probe.file"),
            ("nodes.encodage.v2.workingFile", "nodes.encodage.v2.file"),
            ("mediainfo.video.codec", "mediainfo.video.codec"),
            ("workingFileX", "workingFileX"),
        ];
        for (legacy, expected) in cases {
            assert_eq!(upgrade_legacy_reference(legacy), expected, "{legacy}");
        }
    }

    #[test]
    fn texts_are_converted_and_invalid_or_escaped_braces_stay_literal() {
        assert_eq!(
            upgrade_legacy_references(
                "{{workDir}}/{{ mediaInfo.title }}.mkv",
                LegacyConversion::KnownRoots
            ),
            "{$work.dir}/{$media.title}.mkv"
        );
        for literal in [
            "{{$x}",
            "{{ pas une ref }}",
            "{{}}",
            "{{workDir",
            "{$file.path}",
        ] {
            assert!(
                matches!(
                    upgrade_legacy_references(literal, LegacyConversion::AllPaths),
                    Cow::Borrowed(_)
                ),
                "{literal}"
            );
        }
        assert_eq!(
            upgrade_legacy_references("{{$a} {{b}}", LegacyConversion::AllPaths),
            "{{$a} {$b}"
        );
    }

    #[test]
    fn known_roots_only_leave_foreign_mustaches_literal() {
        assert_eq!(
            upgrade_legacy_references(
                "--id {{id}} {{plugin.var}} {{workDir}} {{nodes.a.message}} {{nodes.a.x}}",
                LegacyConversion::KnownRoots
            ),
            "--id {{id}} {{plugin.var}} {$work.dir} {$nodes.a.message} {{nodes.a.x}}"
        );
        assert_eq!(
            upgrade_legacy_references("{{plugin.var}}", LegacyConversion::AllPaths),
            "{$plugin.var}"
        );
        assert_eq!(
            upgrade_legacy_value(&json!({"args": ["{{id}}", "{{workingFileX}}"]})),
            json!({"args": ["{{id}}", "{{workingFileX}}"]})
        );
    }

    #[test]
    fn conversion_is_idempotent() {
        let once = upgrade_legacy_references(
            "{{workingFile}} {{nodes.a.workingFile}}",
            LegacyConversion::AllPaths,
        );
        let twice = upgrade_legacy_references(&once, LegacyConversion::AllPaths).into_owned();
        assert_eq!(once, twice);
    }

    #[test]
    fn code_and_secrets_are_never_converted() {
        let config = json!({
            "code": "echo {{workDir}}",
            "secrets": {"token": "{{workDir}}"},
            "cible": ["{{workingFile}}", 3]
        });
        assert_eq!(
            upgrade_legacy_value(&config),
            json!({
                "code": "echo {{workDir}}",
                "secrets": {"token": "{{workDir}}"},
                "cible": ["{$file.path}", 3]
            })
        );
    }

    #[test]
    fn graphs_report_whether_they_changed() -> Result<(), serde_json::Error> {
        let mut graph: FlowGraph = serde_json::from_value(json!({
            "nodes": [{"id": "a", "type": "x", "name": "A", "config": {"v": "{{workDir}}"}}]
        }))?;
        assert!(upgrade_legacy_graph(&mut graph));
        assert_eq!(graph.nodes[0].config, json!({"v": "{$work.dir}"}));
        assert!(!upgrade_legacy_graph(&mut graph));
        Ok(())
    }

    #[test]
    fn definitions_are_rewritten_only_when_needed() -> Result<(), serde_json::Error> {
        let legacy = json!({
            "format": 1,
            "nodes": [{"id": "a", "type": "x", "name": "A", "config": {"v": "{{workingFile}}"}}],
            "connections": []
        })
        .to_string();
        let rewritten = upgrade_legacy_definition(&legacy).unwrap_or_default();
        let value: Value = serde_json::from_str(&rewritten)?;
        assert_eq!(value["nodes"][0]["config"]["v"], json!("{$file.path}"));
        assert_eq!(
            value["format"],
            json!(1),
            "les clés inconnues sont conservées"
        );
        assert_eq!(upgrade_legacy_definition(&rewritten), None);
        assert_eq!(upgrade_legacy_definition("pas du json"), None);
        Ok(())
    }
}
