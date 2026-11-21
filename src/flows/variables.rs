//! Résolution des variables déclarées d'un script au moment de l'exécution (ADR 0025, phase B).
//!
//! Seules les clés déclarées dans `runtime.variables` sont traitées : les valeurs par défaut du
//! schéma comblent les clés absentes, les références `{{…}}` sont remplacées, puis chaque valeur
//! est contrôlée selon son type. Le code du script et les autres clés du nœud restent intacts.

use forgearr_shared::{
    exposed_variables::ExposedVariables,
    script_variables::{ScriptVariable, VariableKind, default_from_text},
};
use serde_json::{Map, Number, Value};
use std::{collections::HashMap, path::Path};
use thiserror::Error;

const ROOT_WORKING_FILE: &str = "workingFile";
const ROOT_SOURCE_FILE: &str = "sourceFile";
const ROOT_WORK_DIR: &str = "workDir";
const ROOT_MEDIA_INFO: &str = "mediaInfo";
const ROOT_NODES: &str = "nodes";
const TEMPLATE_OPEN: &str = "{{";
const TEMPLATE_CLOSE: &str = "}}";

/// Ce qu'un nœud déjà exécuté expose aux suivants (`{{nodes.<id>.message|nextPort|workingFile}}`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeOutput {
    pub message: Option<String>,
    pub next_port: Option<String>,
    pub working_file: String,
}

/// Valeurs accessibles par les références d'un nœud, construites une fois par étape.
#[derive(Clone, Debug)]
pub struct VariableContext {
    root: Value,
}

impl VariableContext {
    /// `exposed` : ce que les plugins chargés exposent ; chaque variable se lit à son chemin de
    /// `media_info` et devient accessible sous `espace.nom` quand une valeur y existe.
    pub fn new(
        working_file: &Path,
        source_file: &Path,
        work_dir: &Path,
        media_info: &Value,
        nodes: &HashMap<String, NodeOutput>,
        exposed: &[&ExposedVariables],
    ) -> Self {
        let nodes_json: Map<String, Value> = nodes
            .iter()
            .map(|(id, output)| {
                (
                    id.clone(),
                    serde_json::json!({
                        "message": output.message,
                        "nextPort": output.next_port,
                        "workingFile": output.working_file,
                    }),
                )
            })
            .collect();
        let mut root = Map::new();
        root.insert(ROOT_WORKING_FILE.into(), path_value(working_file));
        root.insert(ROOT_SOURCE_FILE.into(), path_value(source_file));
        root.insert(ROOT_WORK_DIR.into(), path_value(work_dir));
        root.insert(ROOT_MEDIA_INFO.into(), media_info.clone());
        root.insert(ROOT_NODES.into(), Value::Object(nodes_json));
        for exposure in exposed {
            for variable in &exposure.variables {
                if let Some(value) = lookup_in(media_info, &variable.path) {
                    insert_nested(&mut root, &exposure.namespace, &variable.name, value.clone());
                }
            }
        }
        Self {
            root: Value::Object(root),
        }
    }

    /// Valeur désignée par un chemin pointé ; `null` compte comme absente.
    fn lookup(&self, path: &str) -> Option<&Value> {
        lookup_in(&self.root, path)
    }
}

/// Références `{{chemin}}` d'un texte, dans l'ordre.
pub(super) fn references_in(text: &str) -> Vec<&str> {
    parse_template(text)
        .into_iter()
        .filter_map(|segment| match segment {
            Segment::Reference(path) => Some(path),
            Segment::Text(_) => None,
        })
        .collect()
}

/// Parcourt `root` selon un chemin pointé (clés d'objet, indices de tableau) ; `null` compte comme absente.
pub(super) fn lookup_in<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for segment in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    (!current.is_null()).then_some(current)
}

/// Place `value` sous `espace.a.b` en créant les objets intermédiaires.
fn insert_nested(root: &mut Map<String, Value>, namespace: &str, name: &str, value: Value) {
    let segments: Vec<&str> = name.split('.').collect();
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut current = root
        .entry(namespace.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    for segment in parents {
        let Value::Object(map) = current else {
            return;
        };
        current = map
            .entry((*segment).to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if let Value::Object(map) = current {
        map.insert((*last).to_owned(), value);
    }
}

fn path_value(path: &Path) -> Value {
    Value::String(path.to_string_lossy().into_owned())
}

/// Raison pour laquelle les variables d'un nœud n'ont pas pu être résolues.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VariableError {
    #[error("variable « {variable} » : la référence {{{{{reference}}}}} n'a pas de valeur à cette étape")]
    MissingReference { variable: String, reference: String },
    #[error("variable « {0} » requise : aucune valeur")]
    MissingRequired(String),
    #[error("variable « {variable} » : une valeur de type {expected} est attendue")]
    WrongType {
        variable: String,
        expected: &'static str,
    },
    #[error("variable « {0} » : valeur hors des bornes")]
    OutOfRange(String),
    #[error("variable « {0} » : valeur absente des options du choix")]
    NotAnOption(String),
}

/// Fragment d'un texte : littéral, ou référence `{{chemin}}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Segment<'a> {
    Text(&'a str),
    Reference(&'a str),
}

fn is_reference(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Découpe un texte ; `{{…}}` dont le contenu n'est pas un chemin valide reste littéral.
fn parse_template(text: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut literal_from = 0;
    let mut cursor = 0;
    while let Some(open_offset) = text[cursor..].find(TEMPLATE_OPEN) {
        let open = cursor + open_offset;
        let inner_start = open + TEMPLATE_OPEN.len();
        let Some(close_offset) = text[inner_start..].find(TEMPLATE_CLOSE) else {
            break;
        };
        let close = inner_start + close_offset;
        let reference = text[inner_start..close].trim();
        if is_reference(reference) {
            if literal_from < open {
                segments.push(Segment::Text(&text[literal_from..open]));
            }
            segments.push(Segment::Reference(reference));
            literal_from = close + TEMPLATE_CLOSE.len();
            cursor = literal_from;
        } else {
            cursor = inner_start;
        }
    }
    if literal_from < text.len() {
        segments.push(Segment::Text(&text[literal_from..]));
    }
    segments
}

pub(super) fn as_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Remplace les références d'une valeur ; `Err` porte la première référence sans valeur.
fn resolve_value(value: &Value, context: &VariableContext) -> Result<Value, String> {
    match value {
        Value::String(text) => {
            let segments = parse_template(text);
            if let [Segment::Reference(path)] = segments.as_slice() {
                return context
                    .lookup(path)
                    .cloned()
                    .ok_or_else(|| (*path).to_owned());
            }
            let mut out = String::new();
            for segment in segments {
                match segment {
                    Segment::Text(literal) => out.push_str(literal),
                    Segment::Reference(path) => {
                        let found = context.lookup(path).ok_or_else(|| path.to_owned())?;
                        out.push_str(&as_text(found));
                    }
                }
            }
            Ok(Value::String(out))
        }
        Value::Array(items) => items
            .iter()
            .map(|item| resolve_value(item, context))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, item)| resolve_value(item, context).map(|resolved| (key.clone(), resolved)))
            .collect::<Result<Map<_, _>, _>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

fn number_in_range(variable: &ScriptVariable, number: &Number) -> bool {
    let Some(value) = number.as_f64() else {
        return false;
    };
    variable
        .min
        .as_ref()
        .and_then(Number::as_f64)
        .is_none_or(|low| value >= low)
        && variable
            .max
            .as_ref()
            .and_then(Number::as_f64)
            .is_none_or(|high| value <= high)
}

/// Contrôle la valeur résolue selon le type de la variable, en convertissant les textes
/// qui représentent un nombre ou un booléen (cas d'une référence insérée dans un texte).
fn coerce(variable: &ScriptVariable, value: Value) -> Result<Value, VariableError> {
    let wrong = |expected| VariableError::WrongType {
        variable: variable.name.clone(),
        expected,
    };
    match variable.kind {
        VariableKind::Text | VariableKind::Multiline | VariableKind::Secret => match value {
            Value::String(_) => Ok(value),
            Value::Number(_) | Value::Bool(_) => Ok(Value::String(as_text(&value))),
            _ => Err(wrong("texte")),
        },
        VariableKind::Number => {
            let parsed = match value {
                Value::Number(_) => value,
                Value::String(text) => default_from_text(VariableKind::Number, &text)
                    .ok()
                    .flatten()
                    .ok_or_else(|| wrong("nombre"))?,
                _ => return Err(wrong("nombre")),
            };
            let Value::Number(number) = &parsed else {
                return Err(wrong("nombre"));
            };
            if number_in_range(variable, number) {
                Ok(parsed)
            } else {
                Err(VariableError::OutOfRange(variable.name.clone()))
            }
        }
        VariableKind::Boolean => match value {
            Value::Bool(_) => Ok(value),
            Value::String(text) => default_from_text(VariableKind::Boolean, &text)
                .ok()
                .flatten()
                .ok_or_else(|| wrong("booléen")),
            _ => Err(wrong("booléen")),
        },
        VariableKind::Choice => match value {
            Value::String(choice) if variable.options.contains(&choice) => {
                Ok(Value::String(choice))
            }
            Value::String(_) => Err(VariableError::NotAnOption(variable.name.clone())),
            _ => Err(wrong("choix")),
        },
    }
}

fn is_blank(value: &Value) -> bool {
    matches!(value, Value::String(text) if text.trim().is_empty())
}

/// Résout une variable : valeur du nœud, sinon défaut du schéma ; une référence sans valeur
/// retombe sur le défaut quand il existe.
fn resolve_variable(
    variable: &ScriptVariable,
    provided: Option<&Value>,
    context: &VariableContext,
) -> Result<Option<Value>, VariableError> {
    let missing = |reference: String| VariableError::MissingReference {
        variable: variable.name.clone(),
        reference,
    };
    let resolved = match (provided, &variable.default_value) {
        (Some(value), Some(default)) => {
            resolve_value(value, context).or_else(|_| resolve_value(default, context))
        }
        (Some(value), None) => resolve_value(value, context),
        (None, Some(default)) => resolve_value(default, context),
        (None, None) => return finish(variable, None),
    }
    .map_err(missing)?;
    finish(variable, Some(resolved))
}

fn finish(
    variable: &ScriptVariable,
    value: Option<Value>,
) -> Result<Option<Value>, VariableError> {
    match value {
        Some(value) if variable.required && is_blank(&value) => {
            Err(VariableError::MissingRequired(variable.name.clone()))
        }
        Some(value) => coerce(variable, value).map(Some),
        None if variable.required => Err(VariableError::MissingRequired(variable.name.clone())),
        None => Ok(None),
    }
}

/// Config du nœud avec les variables déclarées résolues et contrôlées.
///
/// Les clés non déclarées (code en ligne, `interpreter`, `scriptId`…) sont conservées telles quelles.
pub fn resolve_node_config(
    variables: &[ScriptVariable],
    node_config: &Value,
    context: &VariableContext,
) -> Result<Value, VariableError> {
    if variables.is_empty() {
        return Ok(node_config.clone());
    }
    let mut config = node_config.as_object().cloned().unwrap_or_default();
    for variable in variables {
        let provided = config.get(&variable.name).filter(|value| !value.is_null());
        match resolve_variable(variable, provided, context)? {
            Some(value) => {
                config.insert(variable.name.clone(), value);
            }
            None => {
                config.remove(&variable.name);
            }
        }
    }
    Ok(Value::Object(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context() -> VariableContext {
        let mut nodes = HashMap::new();
        nodes.insert(
            "probe".to_owned(),
            NodeOutput {
                message: Some("ok".into()),
                next_port: Some("hevc".into()),
                working_file: "/work/film.mkv".into(),
            },
        );
        VariableContext::new(
            Path::new("/work/film.mkv"),
            Path::new("/in/source.mkv"),
            Path::new("/work"),
            &json!({"video": {"codec": "hevc", "width": 1920}, "streams": [{"lang": "fr"}]}),
            &nodes,
            &[],
        )
    }

    fn variable(name: &str, kind: VariableKind) -> ScriptVariable {
        ScriptVariable {
            name: name.into(),
            kind,
            ..ScriptVariable::default()
        }
    }

    fn resolve(variables: &[ScriptVariable], config: Value) -> Result<Value, VariableError> {
        resolve_node_config(variables, &config, &context())
    }

    #[test]
    fn template_parsing_keeps_literals_and_invalid_markers() {
        assert_eq!(
            parse_template("a {{ workDir }}/b {{x}}"),
            vec![
                Segment::Text("a "),
                Segment::Reference("workDir"),
                Segment::Text("/b "),
                Segment::Reference("x"),
            ]
        );
        assert_eq!(parse_template("{{ pas une ref }}"), vec![Segment::Text("{{ pas une ref }}")]);
        assert_eq!(parse_template("ouvert {{ sans fin"), vec![Segment::Text("ouvert {{ sans fin")]);
        assert_eq!(parse_template("{{}}"), vec![Segment::Text("{{}}")]);
        assert!(parse_template("").is_empty());
    }

    #[test]
    fn a_whole_reference_keeps_its_json_type_and_embedded_ones_become_text() -> Result<(), VariableError> {
        let variables = [
            variable("largeur", VariableKind::Number),
            variable("fichier", VariableKind::Text),
            variable("titre", VariableKind::Text),
        ];
        let resolved = resolve(
            &variables,
            json!({
                "largeur": "{{mediaInfo.video.width}}",
                "fichier": "{{workingFile}}",
                "titre": "{{mediaInfo.video.codec}}-{{mediaInfo.video.width}}",
            }),
        )?;
        assert_eq!(resolved["largeur"], json!(1920));
        assert_eq!(resolved["fichier"], json!("/work/film.mkv"));
        assert_eq!(resolved["titre"], json!("hevc-1920"));
        Ok(())
    }

    #[test]
    fn references_reach_the_flow_values_array_items_and_previous_nodes() -> Result<(), VariableError> {
        let variables = [
            variable("source", VariableKind::Text),
            variable("dossier", VariableKind::Text),
            variable("langue", VariableKind::Text),
            variable("port", VariableKind::Text),
            variable("precedent", VariableKind::Text),
        ];
        let resolved = resolve(
            &variables,
            json!({
                "source": "{{sourceFile}}",
                "dossier": "{{workDir}}",
                "langue": "{{mediaInfo.streams.0.lang}}",
                "port": "{{nodes.probe.nextPort}}",
                "precedent": "{{nodes.probe.workingFile}}",
            }),
        )?;
        assert_eq!(resolved["source"], json!("/in/source.mkv"));
        assert_eq!(resolved["dossier"], json!("/work"));
        assert_eq!(resolved["langue"], json!("fr"));
        assert_eq!(resolved["port"], json!("hevc"));
        assert_eq!(resolved["precedent"], json!("/work/film.mkv"));
        Ok(())
    }

    #[test]
    fn references_are_resolved_inside_nested_values() {
        let resolved = resolve_value(
            &json!({"liste": ["{{workDir}}", 3], "texte": "x"}),
            &context(),
        );
        assert_eq!(resolved, Ok(json!({"liste": ["/work", 3], "texte": "x"})));
        assert_eq!(
            resolve_value(&json!(["{{absent.chemin}}"]), &context()),
            Err("absent.chemin".to_owned())
        );
    }

    #[test]
    fn a_missing_reference_fails_unless_the_variable_has_a_default() -> Result<(), VariableError> {
        let plain = variable("langue", VariableKind::Text);
        assert_eq!(
            resolve(&[plain], json!({"langue": "{{mediaInfo.audio.lang}}"})),
            Err(VariableError::MissingReference {
                variable: "langue".into(),
                reference: "mediaInfo.audio.lang".into(),
            })
        );
        let mut with_default = variable("langue", VariableKind::Text);
        with_default.default_value = Some(json!("fr"));
        let resolved = resolve(&[with_default], json!({"langue": "{{mediaInfo.audio.lang}}"}))?;
        assert_eq!(resolved["langue"], json!("fr"));
        Ok(())
    }

    #[test]
    fn a_null_value_counts_as_missing() {
        let nodes = HashMap::from([("a".to_owned(), NodeOutput::default())]);
        let context = VariableContext::new(Path::new("f"), Path::new("s"), Path::new("w"), &json!({}), &nodes, &[]);
        assert!(context.lookup("nodes.a.message").is_none());
        assert!(context.lookup("nodes.a.workingFile").is_some());
        assert!(context.lookup("nodes.absent.message").is_none());
    }

    #[test]
    fn schema_defaults_fill_absent_keys_and_required_variables_must_have_a_value() -> Result<(), VariableError> {
        let mut crf = variable("crf", VariableKind::Number);
        crf.default_value = Some(json!(23));
        let mut cible = variable("cible", VariableKind::Text);
        cible.required = true;
        let resolved = resolve(&[crf.clone()], json!({}))?;
        assert_eq!(resolved["crf"], json!(23));
        assert_eq!(
            resolve(&[cible.clone()], json!({})),
            Err(VariableError::MissingRequired("cible".into()))
        );
        assert_eq!(
            resolve(&[cible.clone()], json!({"cible": "  "})),
            Err(VariableError::MissingRequired("cible".into()))
        );
        assert_eq!(resolve(&[cible], json!({"cible": "x"}))?["cible"], json!("x"));
        assert_eq!(resolve(&[crf], json!({"crf": 18}))?["crf"], json!(18));
        Ok(())
    }

    #[test]
    fn values_are_checked_and_converted_by_type() -> Result<(), VariableError> {
        let mut crf = variable("crf", VariableKind::Number);
        crf.min = Some(0.into());
        crf.max = Some(51.into());
        assert_eq!(resolve(&[crf.clone()], json!({"crf": "23"}))?["crf"], json!(23));
        assert_eq!(
            resolve(&[crf.clone()], json!({"crf": 99})),
            Err(VariableError::OutOfRange("crf".into()))
        );
        assert!(matches!(
            resolve(&[crf], json!({"crf": "beaucoup"})),
            Err(VariableError::WrongType { .. })
        ));

        let flag = variable("supprimer", VariableKind::Boolean);
        assert_eq!(resolve(&[flag.clone()], json!({"supprimer": "true"}))?["supprimer"], json!(true));
        assert!(resolve(&[flag], json!({"supprimer": 3})).is_err());

        let mut langue = variable("langue", VariableKind::Choice);
        langue.options = vec!["fr".into(), "en".into()];
        assert_eq!(resolve(&[langue.clone()], json!({"langue": "en"}))?["langue"], json!("en"));
        assert_eq!(
            resolve(&[langue], json!({"langue": "de"})),
            Err(VariableError::NotAnOption("langue".into()))
        );
        Ok(())
    }

    #[test]
    fn undeclared_keys_and_inline_code_are_never_touched() -> Result<(), VariableError> {
        let variables = [variable("titre", VariableKind::Text)];
        let config = json!({
            "titre": "{{workDir}}",
            "code": "echo '{{workingFile}}'",
            "interpreter": "sh",
        });
        let resolved = resolve(&variables, config)?;
        assert_eq!(resolved["titre"], json!("/work"));
        assert_eq!(resolved["code"], json!("echo '{{workingFile}}'"));
        assert_eq!(resolved["interpreter"], json!("sh"));
        let untouched = json!({"x": "{{workDir}}"});
        assert_eq!(resolve(&[], untouched.clone())?, untouched);
        Ok(())
    }

    #[test]
    fn an_optional_variable_without_value_stays_absent() -> Result<(), VariableError> {
        let resolved = resolve(&[variable("note", VariableKind::Text)], json!({"note": null}))?;
        assert!(resolved.get("note").is_none());
        Ok(())
    }

    #[test]
    fn errors_name_the_variable_in_french() {
        let message = VariableError::MissingReference {
            variable: "langue".into(),
            reference: "mediaInfo.audio.lang".into(),
        }
        .to_string();
        assert!(message.contains("langue") && message.contains("{{mediaInfo.audio.lang}}"));
    }

    fn exposure() -> ExposedVariables {
        use forgearr_shared::exposed_variables::ExposedVariable;
        ExposedVariables {
            namespace: "mediainfo".into(),
            contract_version: "1.0.0".into(),
            variables: vec![
                ExposedVariable {
                    name: "video.codec".into(),
                    path: "video.codec".into(),
                    ..ExposedVariable::default()
                },
                ExposedVariable {
                    name: "video.largeur".into(),
                    path: "video.width".into(),
                    ..ExposedVariable::default()
                },
                ExposedVariable {
                    name: "audio.langue".into(),
                    path: "streams.0.lang".into(),
                    ..ExposedVariable::default()
                },
                ExposedVariable {
                    name: "absente".into(),
                    path: "n.existe.pas".into(),
                    ..ExposedVariable::default()
                },
            ],
        }
    }

    #[test]
    fn exposed_variables_are_readable_under_their_namespace() -> Result<(), VariableError> {
        let media = json!({"video": {"codec": "hevc", "width": 1920}, "streams": [{"lang": "fr"}]});
        let exposed = exposure();
        let context = VariableContext::new(
            Path::new("f"),
            Path::new("s"),
            Path::new("w"),
            &media,
            &HashMap::new(),
            &[&exposed],
        );
        let variables = [
            variable("codec", VariableKind::Text),
            variable("largeur", VariableKind::Number),
            variable("langue", VariableKind::Text),
            variable("absente", VariableKind::Text),
        ];
        let resolved = resolve_node_config(
            &variables[..3],
            &json!({
                "codec": "{{mediainfo.video.codec}}",
                "largeur": "{{mediainfo.video.largeur}}",
                "langue": "{{mediainfo.audio.langue}}",
            }),
            &context,
        )?;
        assert_eq!(resolved["codec"], json!("hevc"));
        assert_eq!(resolved["largeur"], json!(1920));
        assert_eq!(resolved["langue"], json!("fr"));
        assert_eq!(
            resolve_node_config(&variables[3..], &json!({"absente": "{{mediainfo.absente}}"}), &context),
            Err(VariableError::MissingReference {
                variable: "absente".into(),
                reference: "mediainfo.absente".into(),
            })
        );
        Ok(())
    }

    #[test]
    fn insert_nested_builds_intermediate_objects_without_overwriting_siblings() {
        let mut root = Map::new();
        insert_nested(&mut root, "ns", "a.b", json!(1));
        insert_nested(&mut root, "ns", "a.c", json!(2));
        insert_nested(&mut root, "ns", "d", json!(3));
        assert_eq!(Value::Object(root), json!({"ns": {"a": {"b": 1, "c": 2}, "d": 3}}));
    }
}
