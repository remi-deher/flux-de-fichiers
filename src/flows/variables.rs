//! Résolution des variables déclarées d'un script au moment de l'exécution (ADR 0025, phase B ;
//! modèle unique `{$…}` de l'ADR 0026).
//!
//! Une seule portée ([`NodeScope`]) et une seule passe : la configuration brute du nœud est rendue
//! une fois. Les clés déclarées dans `runtime.variables` sont rendues strictement (une expression
//! connue sans valeur ni défaut fait échouer le bloc), comblées par les défauts du schéma, puis
//! contrôlées selon leur type. Le code du script et ses secrets restent intacts.

use super::substitution::substitute_node_config;
use crate::secrets::is_encrypted;
use forgearr_shared::{
    exposed_variables::{ExposedVariable, ExposedVariables},
    flows::{
        FlowVariables, extract_json_values,
        legacy_references::{LegacyConversion, upgrade_legacy_strings},
        scalar_to_string,
        template_scope::{
            NodeOutputField, PATH_SEPARATOR, ScopeNamespace, TemplateScope, media_path,
            render_in_scope_strict, split_node_reference,
        },
    },
    relay::{MAX_RELAYED_MESSAGE_CHARS, RelayedNodeOutput},
    script_variables::{
        SECRET_PLACEHOLDER, SECRETS_CONFIG_KEY, ScriptVariable, VariableKind, default_from_text,
    },
};
use serde_json::{Number, Value};
use std::collections::HashMap;
use thiserror::Error;

/// Ce qu'un nœud déjà exécuté expose aux suivants (`{$nodes.<id>.message|nextPort|file}`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeOutput {
    pub message: Option<String>,
    pub next_port: Option<String>,
    pub working_file: String,
}

impl NodeOutput {
    /// Valeur d'un champ ; un fichier vide (nœud antérieur à un relais) n'a pas de valeur.
    fn field(&self, field: NodeOutputField) -> Option<Value> {
        match field {
            NodeOutputField::Message => self.message.clone(),
            NodeOutputField::NextPort => self.next_port.clone(),
            NodeOutputField::File => {
                (!self.working_file.is_empty()).then(|| self.working_file.clone())
            }
        }
        .map(Value::String)
    }
}

impl From<&RelayedNodeOutput> for NodeOutput {
    /// Le fichier de travail d'un nœud exécuté avant le relais appartient au worker précédent : vide.
    fn from(relayed: &RelayedNodeOutput) -> Self {
        Self {
            message: relayed.message.clone(),
            next_port: relayed.next_port.clone(),
            working_file: String::new(),
        }
    }
}

impl From<&NodeOutput> for RelayedNodeOutput {
    fn from(output: &NodeOutput) -> Self {
        Self {
            message: output
                .message
                .as_ref()
                .map(|text| text.chars().take(MAX_RELAYED_MESSAGE_CHARS).collect()),
            next_port: output.next_port.clone(),
        }
    }
}

/// Portée des modèles d'une étape : variables du flux, métadonnées, sorties des nœuds exécutés et
/// variables exposées par les plugins chargés.
#[derive(Clone, Copy, Debug)]
pub struct NodeScope<'a> {
    variables: &'a FlowVariables,
    media_info: &'a Value,
    nodes: &'a HashMap<String, NodeOutput>,
    exposed: &'a [&'a ExposedVariables],
}

impl<'a> NodeScope<'a> {
    pub fn new(
        variables: &'a FlowVariables,
        media_info: &'a Value,
        nodes: &'a HashMap<String, NodeOutput>,
        exposed: &'a [&'a ExposedVariables],
    ) -> Self {
        Self {
            variables,
            media_info,
            nodes,
            exposed,
        }
    }

    /// Variable exposée désignée par `espace.nom`.
    fn exposed_variable(&self, expr: &str) -> Option<&'a ExposedVariable> {
        self.exposed.iter().find_map(|exposure| {
            let name = expr
                .strip_prefix(exposure.namespace.as_str())?
                .strip_prefix(PATH_SEPARATOR)?;
            exposure
                .variables
                .iter()
                .find(|variable| variable.name == name)
        })
    }
}

impl TemplateScope for NodeScope<'_> {
    fn resolve(&self, expr: &str) -> Option<Value> {
        let expr = expr.trim();
        match ScopeNamespace::builtin(expr) {
            Some(ScopeNamespace::Intrinsic | ScopeNamespace::Custom) => {
                TemplateScope::resolve(self.variables, expr)
            }
            Some(ScopeNamespace::Media) => lookup_in(self.media_info, media_path(expr)?).cloned(),
            Some(ScopeNamespace::Node) => {
                let (node_id, field) = split_node_reference(expr)?;
                self.nodes.get(node_id)?.field(field)
            }
            Some(ScopeNamespace::Http | ScopeNamespace::Plugin) => None,
            None => exposed_value(self.media_info, &self.exposed_variable(expr)?.path).cloned(),
        }
    }

    fn namespace_of(&self, expr: &str) -> Option<ScopeNamespace> {
        ScopeNamespace::builtin(expr).or_else(|| {
            self.exposed_variable(expr.trim())
                .map(|_| ScopeNamespace::Plugin)
        })
    }
}

/// Valeur d'une variable exposée : première valeur non nulle de la requête `path` dans `media_info`.
///
/// La requête est celle des conditions du flux (`a.b`, `liste[0]`, `liste[*]`, `liste[clé=valeur]`) ;
/// à défaut, un chemin pointé à indices (`a.0.b`) est lu comme avant.
pub(super) fn exposed_value<'a>(media_info: &'a Value, path: &str) -> Option<&'a Value> {
    extract_json_values(media_info, path)
        .into_iter()
        .find(|value| !value.is_null())
        .or_else(|| lookup_in(media_info, path))
}

/// Parcourt `root` selon un chemin pointé (clés d'objet, indices de tableau) ; `null` compte comme absente.
pub(super) fn lookup_in<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for segment in path.split(PATH_SEPARATOR) {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    (!current.is_null()).then_some(current)
}

/// Raison pour laquelle les variables d'un nœud n'ont pas pu être résolues.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VariableError {
    #[error(
        "variable « {variable} » : la référence {{${reference}}} n'a pas de valeur à cette étape"
    )]
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
    #[error("variable « {0} » : le secret n'a pas été déchiffré avant l'exécution")]
    SecretNotDecrypted(String),
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
            Value::Number(_) | Value::Bool(_) => Ok(Value::String(scalar_to_string(Some(&value)))),
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

/// Un secret est une valeur littérale : jamais de modèle `{$…}`, jamais de valeur par défaut.
fn resolve_secret(
    variable: &ScriptVariable,
    stored: Option<&Value>,
) -> Result<Option<Value>, VariableError> {
    match stored
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        Some(text) if is_encrypted(text) => {
            Err(VariableError::SecretNotDecrypted(variable.name.clone()))
        }
        Some(text) => Ok(Some(Value::String(text.to_owned()))),
        None if variable.required => Err(VariableError::MissingRequired(variable.name.clone())),
        None => Ok(None),
    }
}

/// Valeurs en clair des variables secrètes d'une `config` résolue (pour les masquer des sorties).
pub fn secret_values(variables: &[ScriptVariable], resolved_config: &Value) -> Vec<String> {
    variables
        .iter()
        .filter(|variable| variable.kind == VariableKind::Secret)
        .filter_map(|variable| resolved_config.get(&variable.name)?.as_str())
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Copie de la `config` d'un nœud où chaque secret saisi ou enregistré est remplacé par un
/// marqueur : un aperçu ne montre jamais la valeur d'un secret.
pub fn mask_secrets_for_preview(variables: &[ScriptVariable], node_config: &Value) -> Value {
    let mut config = node_config.clone();
    let Some(stored) = config
        .get_mut(SECRETS_CONFIG_KEY)
        .and_then(Value::as_object_mut)
    else {
        return config;
    };
    for variable in variables
        .iter()
        .filter(|variable| variable.kind == VariableKind::Secret)
    {
        if let Some(value) = stored.get_mut(&variable.name) {
            *value = Value::String(SECRET_PLACEHOLDER.to_owned());
        }
    }
    config
}

fn is_blank(value: &Value) -> bool {
    matches!(value, Value::String(text) if text.trim().is_empty())
}

/// Résout une variable : valeur du nœud, sinon défaut du schéma ; une expression sans valeur
/// retombe sur le défaut quand il existe. Les anciennes références `{{…}}` (valeurs stockées,
/// défauts des manifestes installés) sont converties avant le rendu.
fn resolve_variable(
    variable: &ScriptVariable,
    provided: Option<&Value>,
    scope: &dyn TemplateScope,
) -> Result<Option<Value>, VariableError> {
    let render = |value: &Value| {
        render_in_scope_strict(
            &upgrade_legacy_strings(value, LegacyConversion::AllPaths),
            scope,
        )
    };
    let resolved = match (provided, &variable.default_value) {
        (Some(value), Some(default)) => render(value).or_else(|_| render(default)),
        (Some(value), None) => render(value),
        (None, Some(default)) => render(default),
        (None, None) => return finish(variable, None),
    }
    .map_err(|reference| VariableError::MissingReference {
        variable: variable.name.clone(),
        reference,
    })?;
    finish(variable, Some(resolved))
}

fn finish(variable: &ScriptVariable, value: Option<Value>) -> Result<Option<Value>, VariableError> {
    match value {
        Some(value) if variable.required && is_blank(&value) => {
            Err(VariableError::MissingRequired(variable.name.clone()))
        }
        Some(value) => coerce(variable, value).map(Some),
        None if variable.required => Err(VariableError::MissingRequired(variable.name.clone())),
        None => Ok(None),
    }
}

/// Config du nœud rendue en une seule passe, avec les variables déclarées résolues et contrôlées.
/// Les secrets viennent de `config.secrets` (valeurs littérales) et sont rangés sous le nom de
/// leur variable. `raw_config` est la configuration stockée du nœud, jamais une config déjà rendue :
/// la valeur d'une variable n'est donc jamais interprétée comme un modèle (pas d'injection de
/// second ordre).
pub fn resolve_node_config(
    variables: &[ScriptVariable],
    raw_config: &Value,
    scope: &dyn TemplateScope,
) -> Result<Value, VariableError> {
    let rendered = substitute_node_config(raw_config, scope);
    if variables.is_empty() {
        return Ok(rendered);
    }
    let mut config = rendered.as_object().cloned().unwrap_or_default();
    config.remove(SECRETS_CONFIG_KEY);
    let stored_secrets = raw_config.get(SECRETS_CONFIG_KEY);
    for variable in variables {
        if variable.kind == VariableKind::Secret {
            let stored = stored_secrets.and_then(|map| map.get(&variable.name));
            match resolve_secret(variable, stored)? {
                Some(value) => config.insert(variable.name.clone(), value),
                None => config.remove(&variable.name),
            };
            continue;
        }
        let provided = raw_config
            .get(&variable.name)
            .filter(|value| !value.is_null());
        match resolve_variable(variable, provided, scope)? {
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
    use forgearr_shared::flows::IntrinsicVariable;
    use serde_json::json;

    struct Fixture {
        variables: FlowVariables,
        media: Value,
        nodes: HashMap<String, NodeOutput>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut variables = FlowVariables::default();
            variables.set_intrinsic(IntrinsicVariable::FilePath, json!("/work/film.mkv"));
            variables.set_intrinsic(IntrinsicVariable::SourcePath, json!("/in/source.mkv"));
            variables.set_intrinsic(IntrinsicVariable::WorkDir, json!("/work"));
            variables.set_intrinsic(IntrinsicVariable::WorkerName, Value::Null);
            let mut nodes = HashMap::new();
            nodes.insert(
                "probe".to_owned(),
                NodeOutput {
                    message: Some("ok".into()),
                    next_port: Some("hevc".into()),
                    working_file: "/work/film.mkv".into(),
                },
            );
            Self {
                variables,
                media: json!({"video": {"codec": "hevc", "width": 1920}, "streams": [{"lang": "fr"}]}),
                nodes,
            }
        }

        fn scope<'a>(&'a self, exposed: &'a [&'a ExposedVariables]) -> NodeScope<'a> {
            NodeScope::new(&self.variables, &self.media, &self.nodes, exposed)
        }

        fn resolve(
            &self,
            variables: &[ScriptVariable],
            config: Value,
        ) -> Result<Value, VariableError> {
            resolve_node_config(variables, &config, &self.scope(&[]))
        }
    }

    fn resolve(variables: &[ScriptVariable], config: Value) -> Result<Value, VariableError> {
        Fixture::new().resolve(variables, config)
    }

    fn variable(name: &str, kind: VariableKind) -> ScriptVariable {
        ScriptVariable {
            name: name.into(),
            kind,
            ..ScriptVariable::default()
        }
    }

    #[test]
    fn a_variable_value_is_never_rendered_a_second_time() -> Result<(), String> {
        let mut fixture = Fixture::new();
        fixture
            .variables
            .set_custom("ref", json!("{$work.dir}"))
            .map_err(|err| err.to_string())?;
        fixture
            .variables
            .set_custom("codec", json!("hevc"))
            .map_err(|err| err.to_string())?;
        let target = [variable("target", VariableKind::Text)];
        let render = |text: &str| -> Result<Value, String> {
            resolve_node_config(&target, &json!({"target": text}), &fixture.scope(&[]))
                .map_err(|err| err.to_string())?
                .get("target")
                .cloned()
                .ok_or_else(|| "target absent".to_owned())
        };
        assert_eq!(render("{$vars.ref}")?, json!("{$work.dir}"));
        assert_eq!(render("{$vars.codec}-{$work.dir}")?, json!("hevc-/work"));
        assert_eq!(render("{$vars.codec}-{{workDir}}")?, json!("hevc-/work"));
        assert_eq!(render("{{$vars.codec}")?, json!("{$vars.codec}"));
        Ok(())
    }

    #[test]
    fn a_whole_reference_keeps_its_json_type_and_embedded_ones_become_text()
    -> Result<(), VariableError> {
        let variables = [
            variable("largeur", VariableKind::Number),
            variable("fichier", VariableKind::Text),
            variable("titre", VariableKind::Text),
        ];
        for (largeur, fichier, titre) in [
            (
                "{{mediaInfo.video.width}}",
                "{{workingFile}}",
                "{{mediaInfo.video.codec}}-{{mediaInfo.video.width}}",
            ),
            (
                "{$media.video.width}",
                "{$file.path}",
                "{$media.video.codec}-{$media.video.width}",
            ),
        ] {
            let resolved = resolve(
                &variables,
                json!({"largeur": largeur, "fichier": fichier, "titre": titre}),
            )?;
            assert_eq!(resolved["largeur"], json!(1920));
            assert_eq!(resolved["fichier"], json!("/work/film.mkv"));
            assert_eq!(resolved["titre"], json!("hevc-1920"));
        }
        Ok(())
    }

    #[test]
    fn references_reach_the_flow_values_array_items_and_previous_nodes() -> Result<(), VariableError>
    {
        let variables = [
            variable("source", VariableKind::Text),
            variable("dossier", VariableKind::Text),
            variable("langue", VariableKind::Text),
            variable("port", VariableKind::Text),
            variable("precedent", VariableKind::Text),
            variable("ancien", VariableKind::Text),
        ];
        let resolved = resolve(
            &variables,
            json!({
                "source": "{$source.path}",
                "dossier": "{$work.dir}",
                "langue": "{$media.streams.0.lang}",
                "port": "{$nodes.probe.nextPort}",
                "precedent": "{$nodes.probe.file}",
                "ancien": "{{nodes.probe.workingFile}}",
            }),
        )?;
        assert_eq!(resolved["source"], json!("/in/source.mkv"));
        assert_eq!(resolved["dossier"], json!("/work"));
        assert_eq!(resolved["langue"], json!("fr"));
        assert_eq!(resolved["port"], json!("hevc"));
        assert_eq!(resolved["precedent"], json!("/work/film.mkv"));
        assert_eq!(resolved["ancien"], json!("/work/film.mkv"));
        Ok(())
    }

    #[test]
    fn a_missing_reference_fails_unless_a_default_exists() -> Result<(), VariableError> {
        let plain = variable("langue", VariableKind::Text);
        assert_eq!(
            resolve(
                std::slice::from_ref(&plain),
                json!({"langue": "{$media.audio.lang}"})
            ),
            Err(VariableError::MissingReference {
                variable: "langue".into(),
                reference: "media.audio.lang".into(),
            })
        );
        assert_eq!(
            resolve(
                std::slice::from_ref(&plain),
                json!({"langue": "{$worker.name}"})
            ),
            Err(VariableError::MissingReference {
                variable: "langue".into(),
                reference: "worker.name".into(),
            }),
            "une valeur nulle compte comme absente"
        );
        assert_eq!(
            resolve(
                std::slice::from_ref(&plain),
                json!({"langue": "{$media.audio.lang|en}"})
            )?["langue"],
            json!("en")
        );
        assert_eq!(
            resolve(&[plain], json!({"langue": "x{$inconnu.y}"})),
            Err(VariableError::MissingReference {
                variable: "langue".into(),
                reference: "inconnu.y".into(),
            }),
            "une expression d'un espace inconnu est manquante, comme dans l'ancien moteur"
        );
        let mut with_default = variable("langue", VariableKind::Text);
        with_default.default_value = Some(json!("fr"));
        let resolved = resolve(&[with_default], json!({"langue": "{$media.audio.lang}"}))?;
        assert_eq!(resolved["langue"], json!("fr"));
        Ok(())
    }

    #[test]
    fn a_legacy_reference_to_an_empty_node_message_gives_empty_text() -> Result<(), VariableError> {
        let mut fixture = Fixture::new();
        fixture.nodes.insert(
            "a".to_owned(),
            NodeOutput {
                message: Some(String::new()),
                next_port: Some(String::new()),
                working_file: String::new(),
            },
        );
        let variables = [
            variable("message", VariableKind::Text),
            variable("port", VariableKind::Text),
        ];
        let resolved = fixture.resolve(
            &variables,
            json!({"message": "{{nodes.a.message}}", "port": "[{{nodes.a.nextPort}}]"}),
        )?;
        assert_eq!(resolved["message"], json!(""));
        assert_eq!(resolved["port"], json!("[]"));
        Ok(())
    }

    #[test]
    fn a_legacy_reference_without_value_still_fails() {
        let plain = variable("x", VariableKind::Text);
        for (legacy, reference) in [
            ("{{inconnu.x}}", "inconnu.x"),
            ("{{plugin.var}}", "plugin.var"),
        ] {
            assert_eq!(
                resolve(std::slice::from_ref(&plain), json!({"x": legacy})),
                Err(VariableError::MissingReference {
                    variable: "x".into(),
                    reference: reference.into(),
                }),
                "{legacy}"
            );
        }
    }

    #[test]
    fn a_legacy_manifest_default_is_resolved() -> Result<(), VariableError> {
        let mut codec = variable("codec", VariableKind::Text);
        codec.default_value = Some(json!("{{mediaInfo.video.codec}}"));
        assert_eq!(resolve(&[codec], json!({}))?["codec"], json!("hevc"));
        Ok(())
    }

    #[test]
    fn missing_node_values_have_no_value() {
        let nodes = HashMap::from([("a".to_owned(), NodeOutput::default())]);
        let variables = FlowVariables::default();
        let media = json!({});
        let scope = NodeScope::new(&variables, &media, &nodes, &[]);
        assert!(scope.resolve("nodes.a.message").is_none());
        assert!(
            scope.resolve("nodes.a.file").is_none(),
            "un fichier vide (nœud antérieur à un relais) n'a pas de valeur"
        );
        assert!(scope.resolve("nodes.absent.message").is_none());
        assert_eq!(
            scope.namespace_of("nodes.absent.message"),
            Some(ScopeNamespace::Node)
        );
    }

    #[test]
    fn schema_defaults_fill_absent_keys_and_required_variables_must_have_a_value()
    -> Result<(), VariableError> {
        let mut crf = variable("crf", VariableKind::Number);
        crf.default_value = Some(json!(23));
        let mut cible = variable("cible", VariableKind::Text);
        cible.required = true;
        let resolved = resolve(std::slice::from_ref(&crf), json!({}))?;
        assert_eq!(resolved["crf"], json!(23));
        assert_eq!(
            resolve(std::slice::from_ref(&cible), json!({})),
            Err(VariableError::MissingRequired("cible".into()))
        );
        assert_eq!(
            resolve(std::slice::from_ref(&cible), json!({"cible": "  "})),
            Err(VariableError::MissingRequired("cible".into()))
        );
        assert_eq!(
            resolve(&[cible], json!({"cible": "x"}))?["cible"],
            json!("x")
        );
        assert_eq!(resolve(&[crf], json!({"crf": 18}))?["crf"], json!(18));
        Ok(())
    }

    #[test]
    fn values_are_checked_and_converted_by_type() -> Result<(), VariableError> {
        let mut crf = variable("crf", VariableKind::Number);
        crf.min = Some(0.into());
        crf.max = Some(51.into());
        assert_eq!(
            resolve(std::slice::from_ref(&crf), json!({"crf": "23"}))?["crf"],
            json!(23)
        );
        assert_eq!(
            resolve(std::slice::from_ref(&crf), json!({"crf": 99})),
            Err(VariableError::OutOfRange("crf".into()))
        );
        assert!(matches!(
            resolve(&[crf], json!({"crf": "beaucoup"})),
            Err(VariableError::WrongType { .. })
        ));

        let flag = variable("supprimer", VariableKind::Boolean);
        assert_eq!(
            resolve(std::slice::from_ref(&flag), json!({"supprimer": "true"}))?["supprimer"],
            json!(true)
        );
        assert!(resolve(&[flag], json!({"supprimer": 3})).is_err());

        let mut langue = variable("langue", VariableKind::Choice);
        langue.options = vec!["fr".into(), "en".into()];
        assert_eq!(
            resolve(std::slice::from_ref(&langue), json!({"langue": "en"}))?["langue"],
            json!("en")
        );
        assert_eq!(
            resolve(&[langue], json!({"langue": "de"})),
            Err(VariableError::NotAnOption("langue".into()))
        );
        Ok(())
    }

    #[test]
    fn undeclared_keys_are_rendered_once_and_inline_code_is_never_touched()
    -> Result<(), VariableError> {
        let variables = [variable("titre", VariableKind::Text)];
        let config = json!({
            "titre": "{{workDir}}",
            "code": "echo '{{workingFile}}' {$file.path}",
            "interpreter": "sh",
            "autre": "{$work.dir}",
        });
        let resolved = resolve(&variables, config)?;
        assert_eq!(resolved["titre"], json!("/work"));
        assert_eq!(
            resolved["code"],
            json!("echo '{{workingFile}}' {$file.path}")
        );
        assert_eq!(resolved["interpreter"], json!("sh"));
        assert_eq!(resolved["autre"], json!("/work"));
        assert_eq!(
            resolve(&[], json!({"x": "{{workDir}}"}))?,
            json!({"x": "/work"})
        );
        Ok(())
    }

    #[test]
    fn an_optional_variable_without_value_stays_absent() -> Result<(), VariableError> {
        let resolved = resolve(
            &[variable("note", VariableKind::Text)],
            json!({"note": null}),
        )?;
        assert!(resolved.get("note").is_none());
        Ok(())
    }

    #[test]
    fn errors_use_the_single_template_syntax() {
        let message = VariableError::MissingReference {
            variable: "langue".into(),
            reference: "media.audio.lang".into(),
        }
        .to_string();
        assert!(message.contains("langue") && message.contains("{$media.audio.lang}"));
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
        let fixture = Fixture::new();
        let exposed = exposure();
        let exposures = [&exposed];
        let scope = fixture.scope(&exposures);
        let variables = [
            variable("codec", VariableKind::Text),
            variable("largeur", VariableKind::Number),
            variable("langue", VariableKind::Text),
            variable("absente", VariableKind::Text),
        ];
        let resolved = resolve_node_config(
            &variables[..3],
            &json!({
                "codec": "{$mediainfo.video.codec}",
                "largeur": "{{mediainfo.video.largeur}}",
                "langue": "{$mediainfo.audio.langue}",
            }),
            &scope,
        )?;
        assert_eq!(resolved["codec"], json!("hevc"));
        assert_eq!(resolved["largeur"], json!(1920));
        assert_eq!(resolved["langue"], json!("fr"));
        assert_eq!(
            resolve_node_config(
                &variables[3..],
                &json!({"absente": "{$mediainfo.absente}"}),
                &scope
            ),
            Err(VariableError::MissingReference {
                variable: "absente".into(),
                reference: "mediainfo.absente".into(),
            })
        );
        Ok(())
    }

    #[test]
    fn exposed_variables_read_engine_query_paths_with_selectors() -> Result<(), VariableError> {
        let mut fixture = Fixture::new();
        fixture.media = json!({"mediainfo": {"media": {"track": [
            {"@type": "General", "Format": "Matroska"},
            {"@type": "Video", "Format": "HEVC", "Width": "1920"},
            {"@type": "Audio", "Format": "AAC"},
            {"@type": "Audio", "Format": "E-AC-3"},
        ]}}});
        let exposed = selector_exposure();
        let exposures = [&exposed];
        let scope = fixture.scope(&exposures);
        let names = [
            ("codec", VariableKind::Text, "{$mediainfo.video.codec}"),
            ("largeur", VariableKind::Number, "{$mediainfo.video.width}"),
            ("audio", VariableKind::Text, "{$mediainfo.audio.codec}"),
            (
                "conteneur",
                VariableKind::Text,
                "{$mediainfo.general.format}",
            ),
            ("premier", VariableKind::Text, "{$mediainfo.any.format}"),
        ];
        let variables: Vec<ScriptVariable> = names
            .iter()
            .map(|(name, kind, _)| variable(name, *kind))
            .collect();
        let config = Value::Object(
            names
                .iter()
                .map(|(name, _, reference)| ((*name).to_owned(), json!(reference)))
                .collect(),
        );
        let resolved = resolve_node_config(&variables, &config, &scope)?;
        assert_eq!(resolved["codec"], json!("HEVC"));
        assert_eq!(resolved["largeur"], json!(1920));
        assert_eq!(
            resolved["audio"],
            json!("AAC"),
            "la première piste audio l'emporte"
        );
        assert_eq!(resolved["conteneur"], json!("Matroska"));
        assert_eq!(resolved["premier"], json!("Matroska"));
        assert_eq!(
            resolve_node_config(
                &[variable("sous_titres", VariableKind::Text)],
                &json!({"sous_titres": "{$mediainfo.text.format}"}),
                &scope,
            ),
            Err(VariableError::MissingReference {
                variable: "sous_titres".into(),
                reference: "mediainfo.text.format".into(),
            }),
            "une requête sans résultat n'a pas de valeur"
        );
        Ok(())
    }

    fn selector_exposure() -> ExposedVariables {
        use forgearr_shared::exposed_variables::ExposedVariable;
        let track = |name: &str, field: &str, selector: &str| ExposedVariable {
            name: name.into(),
            path: format!("mediainfo.media.track{selector}.{field}"),
            ..ExposedVariable::default()
        };
        ExposedVariables {
            namespace: "mediainfo".into(),
            contract_version: "1.0.0".into(),
            variables: vec![
                track("video.codec", "Format", "[@type=Video]"),
                track("video.width", "Width", "[@type=Video]"),
                track("audio.codec", "Format", "[@type=Audio]"),
                track("general.format", "Format", "[0]"),
                track("any.format", "Format", "[*]"),
                track("text.format", "Format", "[@type=Text]"),
            ],
        }
    }

    #[test]
    fn a_null_match_is_skipped_for_the_next_non_null_one() {
        let media = json!({"t": [{"k": null}, {"k": "x"}]});
        assert_eq!(exposed_value(&media, "t[*].k"), Some(&json!("x")));
        assert_eq!(exposed_value(&media, "t[0].k"), None);
        assert_eq!(
            exposed_value(&media, "t.1.k"),
            Some(&json!("x")),
            "chemin pointé à indices"
        );
    }

    fn secret(name: &str, required: bool) -> ScriptVariable {
        ScriptVariable {
            required,
            ..variable(name, VariableKind::Secret)
        }
    }

    #[test]
    fn a_secret_is_read_literally_from_the_secrets_object_and_never_resolved() {
        let config = resolve(
            &[secret("jeton", true)],
            json!({"secrets": {"jeton": "{$file.path}"}, "jeton": "ignoré", "autre": 1}),
        );
        assert_eq!(
            config,
            Ok(json!({"jeton": "{$file.path}", "autre": 1})),
            "littéral, hors `secrets`, clé de premier niveau écrasée"
        );
    }

    #[test]
    fn a_missing_secret_fails_when_required_and_is_omitted_otherwise() {
        assert_eq!(
            resolve(&[secret("jeton", true)], json!({"secrets": {"jeton": ""}})),
            Err(VariableError::MissingRequired("jeton".into()))
        );
        assert_eq!(
            resolve(&[secret("jeton", false)], json!({"jeton": "fuite"})),
            Ok(json!({}))
        );
    }

    #[test]
    fn a_secret_still_encrypted_is_refused_instead_of_reaching_the_script() {
        assert_eq!(
            resolve(
                &[secret("jeton", false)],
                json!({"secrets": {"jeton": "enc:1:abc:def"}})
            ),
            Err(VariableError::SecretNotDecrypted("jeton".into()))
        );
    }

    #[test]
    fn secret_values_lists_only_the_non_empty_secrets_of_a_resolved_config() {
        let variables = [
            secret("jeton", false),
            secret("vide", false),
            variable("cible", VariableKind::Text),
        ];
        let resolved = json!({"jeton": "t0k", "vide": "", "cible": "visible"});
        assert_eq!(secret_values(&variables, &resolved), vec!["t0k".to_owned()]);
    }

    #[test]
    fn the_preview_config_replaces_every_secret_by_a_placeholder() {
        let variables = [secret("jeton", true), secret("autre", false)];
        let masked = mask_secrets_for_preview(
            &variables,
            &json!({"secrets": {"jeton": "t0k", "autre": ""}, "cible": "x"}),
        );
        assert_eq!(
            masked,
            json!({"secrets": {"jeton": SECRET_PLACEHOLDER, "autre": SECRET_PLACEHOLDER}, "cible": "x"})
        );
        let untouched = json!({"cible": "x"});
        assert_eq!(mask_secrets_for_preview(&variables, &untouched), untouched);
    }

    #[test]
    fn a_relayed_output_keeps_its_message_and_port_but_not_the_previous_workers_file() {
        let relayed = RelayedNodeOutput {
            message: Some("analysé".into()),
            next_port: Some("hevc".into()),
        };
        let restored = NodeOutput::from(&relayed);
        assert_eq!(restored.message.as_deref(), Some("analysé"));
        assert_eq!(restored.next_port.as_deref(), Some("hevc"));
        assert!(restored.working_file.is_empty());

        let mut fixture = Fixture::new();
        fixture.nodes.insert("probe".to_owned(), restored);
        let config = resolve_node_config(
            &[
                variable("a", VariableKind::Text),
                variable("b", VariableKind::Text),
            ],
            &json!({"a": "{$nodes.probe.message}", "b": "{$nodes.probe.file}"}),
            &fixture.scope(&[]),
        );
        assert_eq!(
            config,
            Err(VariableError::MissingReference {
                variable: "b".into(),
                reference: "nodes.probe.file".into()
            }),
            "le fichier d'un nœud antérieur au relais n'a pas de valeur"
        );
    }

    #[test]
    fn a_node_output_sent_with_a_relay_is_bounded_in_length() {
        let long = "é".repeat(MAX_RELAYED_MESSAGE_CHARS + 50);
        let relayed = RelayedNodeOutput::from(&NodeOutput {
            message: Some(long),
            next_port: None,
            working_file: "/work/x".into(),
        });
        assert_eq!(
            relayed.message.map(|text| text.chars().count()),
            Some(MAX_RELAYED_MESSAGE_CHARS)
        );
        assert_eq!(relayed.next_port, None);
    }

    #[test]
    fn a_node_id_may_contain_dots() {
        let nodes = HashMap::from([(
            "encodage.v2".to_owned(),
            NodeOutput {
                message: Some("fini".into()),
                next_port: None,
                working_file: "/w/x.mkv".into(),
            },
        )]);
        let variables = FlowVariables::default();
        let media = json!({});
        let scope = NodeScope::new(&variables, &media, &nodes, &[]);
        assert_eq!(
            scope.resolve("nodes.encodage.v2.message"),
            Some(json!("fini"))
        );
        assert_eq!(
            scope.resolve("nodes.encodage.v2.file"),
            Some(json!("/w/x.mkv"))
        );
        assert!(scope.resolve("nodes.encodage.v2.nextPort").is_none());
    }
}
