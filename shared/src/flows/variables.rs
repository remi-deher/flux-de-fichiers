//! Variables de flux : contexte par tâche et rendu des modèles `{$nom|défaut}`.
//!
//! Une seule syntaxe sert à tous les blocs (binaire, script, HTTP) : `{$expr}` est remplacé par
//! la valeur de `expr`, `{$expr|défaut}` utilise `défaut` si la valeur est absente ou vide, et
//! `{{$` produit le littéral `{$`. Les variables personnalisées sont lues sous le préfixe `vars.`,
//! les variables intrinsèques par leur clé (`file.name`, `job.id`…). La portée complète (métadonnées,
//! nœuds, plugins) est décrite dans `template_scope`.

use crate::script_variables::SECRETS_CONFIG_KEY;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{convert::Infallible, fmt};

/// Ouverture d'un jeton de modèle.
pub const TEMPLATE_OPEN: &str = "{$";
/// Accolade supplémentaire qui, placée devant `{$`, en fait un littéral.
pub const TEMPLATE_ESCAPE_PREFIX: char = '{';
/// Fermeture d'un jeton de modèle.
pub const TEMPLATE_CLOSE: char = '}';
/// Séparateur entre l'expression et sa valeur par défaut.
pub const DEFAULT_SEPARATOR: char = '|';
/// Préfixe des variables personnalisées dans une expression.
pub const CUSTOM_PREFIX: &str = "vars.";
/// Clé racine des variables personnalisées dans le document exposé aux scripts et conditions.
pub const CUSTOM_NAMESPACE: &str = "vars";
/// Nombre maximal de variables personnalisées par tâche.
pub const MAX_CUSTOM_VARIABLES: usize = 64;
/// Longueur maximale d'un nom de variable personnalisée.
pub const MAX_VARIABLE_NAME_LEN: usize = 64;
/// Taille maximale (JSON sérialisé) d'une valeur de variable personnalisée.
pub const MAX_VARIABLE_VALUE_BYTES: usize = 4096;
/// Identifiant du bloc primitif « Définir une variable » (suffixe des types de bloc associés).
pub const SET_VARIABLE_BLOCK: &str = "set-variable";
/// Clés de configuration jamais rendues ni converties : le code d'un script et ses secrets
/// stockés sont transmis tels quels.
pub const RAW_CONFIG_KEYS: [&str; 2] = ["code", SECRETS_CONFIG_KEY];
/// Espaces de noms propres aux blocs HTTP (`var.` : variables locales extraites entre étapes).
pub const HTTP_NAMESPACES: [&str; 4] = ["context.", "var.", "setting.", "config."];

/// Variables intrinsèques, renseignées par le moteur et en lecture seule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntrinsicVariable {
    #[serde(rename = "file.name")]
    FileName,
    #[serde(rename = "file.stem")]
    FileStem,
    #[serde(rename = "file.ext")]
    FileExt,
    #[serde(rename = "file.dir")]
    FileDir,
    #[serde(rename = "file.size")]
    FileSize,
    /// Chemin complet du fichier de travail (ancien `{{workingFile}}`).
    #[serde(rename = "file.path")]
    FilePath,
    #[serde(rename = "source.name")]
    SourceName,
    /// Chemin complet du fichier source (ancien `{{sourceFile}}`).
    #[serde(rename = "source.path")]
    SourcePath,
    /// Dossier de travail de la tâche (ancien `{{workDir}}`).
    #[serde(rename = "work.dir")]
    WorkDir,
    #[serde(rename = "library.name")]
    LibraryName,
    #[serde(rename = "library.id")]
    LibraryId,
    #[serde(rename = "worker.name")]
    WorkerName,
    #[serde(rename = "job.id")]
    JobId,
    #[serde(rename = "flow.id")]
    FlowId,
    /// Code d'erreur de la tâche, renseigné seulement pendant le flux d'échec.
    #[serde(rename = "error.code")]
    ErrorCode,
    /// Message d'erreur de la tâche, renseigné seulement pendant le flux d'échec.
    #[serde(rename = "error.message")]
    ErrorMessage,
}

impl IntrinsicVariable {
    pub const ALL: [Self; 16] = [
        Self::FileName,
        Self::FileStem,
        Self::FileExt,
        Self::FileDir,
        Self::FileSize,
        Self::FilePath,
        Self::SourceName,
        Self::SourcePath,
        Self::WorkDir,
        Self::LibraryName,
        Self::LibraryId,
        Self::WorkerName,
        Self::JobId,
        Self::FlowId,
        Self::ErrorCode,
        Self::ErrorMessage,
    ];

    /// Clé stable utilisée dans les modèles (`file.name`…).
    pub const fn key(self) -> &'static str {
        match self {
            Self::FileName => "file.name",
            Self::FileStem => "file.stem",
            Self::FileExt => "file.ext",
            Self::FileDir => "file.dir",
            Self::FileSize => "file.size",
            Self::FilePath => "file.path",
            Self::SourceName => "source.name",
            Self::SourcePath => "source.path",
            Self::WorkDir => "work.dir",
            Self::LibraryName => "library.name",
            Self::LibraryId => "library.id",
            Self::WorkerName => "worker.name",
            Self::JobId => "job.id",
            Self::FlowId => "flow.id",
            Self::ErrorCode => "error.code",
            Self::ErrorMessage => "error.message",
        }
    }

    /// Libellé affiché par l'éditeur.
    pub const fn label(self) -> &'static str {
        match self {
            Self::FileName => "Nom du fichier de travail",
            Self::FileStem => "Nom du fichier sans extension",
            Self::FileExt => "Extension du fichier (sans le point)",
            Self::FileDir => "Dossier du fichier de travail",
            Self::FileSize => "Taille du fichier (octets)",
            Self::FilePath => "Chemin du fichier de travail",
            Self::SourceName => "Nom logique de la source",
            Self::SourcePath => "Chemin du fichier source",
            Self::WorkDir => "Dossier de travail",
            Self::LibraryName => "Nom de la bibliothèque",
            Self::LibraryId => "Identifiant de la bibliothèque",
            Self::WorkerName => "Nom du worker",
            Self::JobId => "Identifiant de la tâche",
            Self::FlowId => "Identifiant du flux",
            Self::ErrorCode => "Code d'erreur (flux d'échec)",
            Self::ErrorMessage => "Message d'erreur (flux d'échec)",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|var| var.key() == key)
    }

    /// Espace de noms (`file`, `job`…) de la variable.
    pub fn namespace(self) -> &'static str {
        let key = self.key();
        key.split_once('.').map_or(key, |(namespace, _)| namespace)
    }
}

/// Erreur d'écriture d'une variable personnalisée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariableError {
    InvalidName(String),
    Reserved(String),
    NotScalar(String),
    ValueTooLarge(String),
    TooMany,
}

impl fmt::Display for VariableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "nom de variable invalide : {name}"),
            Self::Reserved(name) => write!(f, "variable réservée : {name}"),
            Self::NotScalar(name) => write!(
                f,
                "la variable {name} doit être un texte, un nombre ou un booléen"
            ),
            Self::ValueTooLarge(name) => write!(f, "valeur trop longue pour la variable {name}"),
            Self::TooMany => write!(
                f,
                "trop de variables personnalisées (maximum {MAX_CUSTOM_VARIABLES})"
            ),
        }
    }
}

impl std::error::Error for VariableError {}

/// Contexte de variables d'une tâche en cours d'exécution.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowVariables {
    /// Clé = `IntrinsicVariable::key()`.
    #[serde(default)]
    pub intrinsic: Map<String, Value>,
    /// Variables personnalisées, sans le préfixe `vars.`.
    #[serde(default)]
    pub custom: Map<String, Value>,
}

impl FlowVariables {
    pub fn set_intrinsic(&mut self, var: IntrinsicVariable, value: Value) {
        self.intrinsic.insert(var.key().to_owned(), value);
    }

    /// Écrit une variable personnalisée après validation du nom, du type et de la taille.
    pub fn set_custom(&mut self, name: &str, value: Value) -> Result<(), VariableError> {
        validate_custom(name, &value)?;
        if !self.custom.contains_key(name) && self.custom.len() >= MAX_CUSTOM_VARIABLES {
            return Err(VariableError::TooMany);
        }
        self.custom.insert(name.to_owned(), value);
        Ok(())
    }

    /// Fusionne un lot de variables personnalisées ; rien n'est écrit si une entrée est invalide.
    pub fn merge_custom(&mut self, incoming: &Map<String, Value>) -> Result<(), VariableError> {
        let mut merged = self.clone();
        for (name, value) in incoming {
            merged.set_custom(name, value.clone())?;
        }
        *self = merged;
        Ok(())
    }

    /// Résout `vars.<nom>` ou une clé intrinsèque.
    pub fn lookup(&self, expr: &str) -> Option<&Value> {
        let expr = expr.trim();
        match expr.strip_prefix(CUSTOM_PREFIX) {
            Some(name) => self.custom.get(name),
            None => IntrinsicVariable::from_key(expr).and_then(|var| self.intrinsic.get(var.key())),
        }
    }

    /// Vrai si une propriété de condition désigne une variable plutôt que la sonde.
    pub fn is_variable_property(property: &str) -> bool {
        let property = property.trim();
        property.starts_with(CUSTOM_PREFIX)
            || IntrinsicVariable::ALL.into_iter().any(|var| {
                property
                    .strip_prefix(var.namespace())
                    .is_some_and(|rest| rest.starts_with('.'))
            })
    }

    /// Document imbriqué `{"file": {"name": ..}, "vars": {..}}` pour les scripts et conditions.
    pub fn as_document(&self) -> Value {
        let mut doc = Map::new();
        for (key, value) in &self.intrinsic {
            let Some((namespace, field)) = key.split_once('.') else {
                continue;
            };
            if let Value::Object(group) = doc
                .entry(namespace.to_owned())
                .or_insert_with(|| Value::Object(Map::new()))
            {
                group.insert(field.to_owned(), value.clone());
            }
        }
        doc.insert(
            CUSTOM_NAMESPACE.to_owned(),
            Value::Object(self.custom.clone()),
        );
        Value::Object(doc)
    }
}

fn validate_custom(name: &str, value: &Value) -> Result<(), VariableError> {
    if IntrinsicVariable::from_key(name).is_some() {
        return Err(VariableError::Reserved(name.to_owned()));
    }
    if !is_valid_variable_name(name) {
        return Err(VariableError::InvalidName(name.to_owned()));
    }
    if matches!(value, Value::Array(_) | Value::Object(_)) {
        return Err(VariableError::NotScalar(name.to_owned()));
    }
    if value.to_string().len() > MAX_VARIABLE_VALUE_BYTES {
        return Err(VariableError::ValueTooLarge(name.to_owned()));
    }
    Ok(())
}

/// Nom de variable personnalisée : `[A-Za-z0-9_-]`, 1 à `MAX_VARIABLE_NAME_LEN` caractères.
pub fn is_valid_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_VARIABLE_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Représentation textuelle d'une valeur scalaire (vide pour `null` ou absente).
pub fn scalar_to_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Jeton `{$expr|défaut}` repéré dans un modèle ; `trailing` est le texte qui le suit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateToken<'a> {
    pub expr: &'a str,
    pub default: &'a str,
    pub trailing: &'a str,
}

/// Texte d'un jeton résolu : la valeur, ou la valeur par défaut si elle est absente ou vide.
pub fn render_piece(resolved: Option<&Value>, default: &str) -> String {
    let piece = scalar_to_string(resolved);
    if piece.is_empty() && !default.is_empty() {
        default.to_owned()
    } else {
        piece
    }
}

/// Parcourt un modèle et remplace chaque jeton par le texte que `piece` renvoie.
///
/// `{{$` donne le littéral `{$` ; un `{$` sans fermeture est conservé tel quel.
pub fn render_template_with(
    template: &str,
    piece: &mut dyn FnMut(&TemplateToken<'_>) -> String,
) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find(TEMPLATE_OPEN) {
        let before = &rest[..open];
        let after_open = &rest[open + TEMPLATE_OPEN.len()..];
        if let Some(literal) = before.strip_suffix(TEMPLATE_ESCAPE_PREFIX) {
            rendered.push_str(literal);
            rendered.push_str(TEMPLATE_OPEN);
            rest = after_open;
            continue;
        }
        rendered.push_str(before);
        let Some(close) = after_open.find(TEMPLATE_CLOSE) else {
            rendered.push_str(TEMPLATE_OPEN);
            rest = after_open;
            continue;
        };
        let token = &after_open[..close];
        let trailing = &after_open[close + TEMPLATE_CLOSE.len_utf8()..];
        let (expr, default) = token.split_once(DEFAULT_SEPARATOR).unwrap_or((token, ""));
        rendered.push_str(&piece(&TemplateToken {
            expr: expr.trim(),
            default,
            trailing,
        }));
        rest = trailing;
    }
    rendered.push_str(rest);
    rendered
}

/// Expression d'une chaîne réduite à un seul jeton (`"{$x}"`), dont le type JSON est conservé.
pub fn exact_token(text: &str) -> Option<&str> {
    let inner = text
        .strip_prefix(TEMPLATE_OPEN)?
        .strip_suffix(TEMPLATE_CLOSE)?;
    if inner.contains(TEMPLATE_CLOSE) {
        return None;
    }
    let expr = inner
        .split_once(DEFAULT_SEPARATOR)
        .map_or(inner, |(expr, _)| expr);
    Some(expr.trim())
}

/// Reconstruit un JSON en appliquant `map` à chaque chaîne ; tableaux et objets sont parcourus,
/// les autres scalaires recopiés. La première erreur interrompt le parcours.
pub fn map_strings<E>(
    value: &Value,
    map: &mut dyn FnMut(&str) -> Result<Value, E>,
) -> Result<Value, E> {
    match value {
        Value::String(text) => map(text),
        Value::Array(items) => items
            .iter()
            .map(|item| map_strings(item, map))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, item)| map_strings(item, map).map(|mapped| (key.clone(), mapped)))
            .collect::<Result<Map<_, _>, _>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

/// Variante infaillible de [`map_strings`].
pub fn map_strings_infallible(value: &Value, map: &mut dyn FnMut(&str) -> Value) -> Value {
    map_strings::<Infallible>(value, &mut |text| Ok(map(text)))
        .unwrap_or_else(|never| match never {})
}

/// Expressions citées par un modèle (validation et aide de saisie de l'éditeur).
pub fn template_tokens(template: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    render_template_with(template, &mut |token: &TemplateToken<'_>| {
        tokens.push(token.expr.to_owned());
        String::new()
    });
    tokens
}

/// Vrai si le moteur sait résoudre l'expression sans dépendre des plugins chargés : variable
/// intrinsèque, `vars.<nom>` valide, `media.<chemin>`, `nodes.<id>.<champ>` ou espace HTTP.
pub fn is_known_expression(expr: &str) -> bool {
    super::template_scope::ScopeNamespace::builtin(expr).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_strings_rebuilds_nested_json_and_stops_on_the_first_error() {
        let value = serde_json::json!({"a": ["x", 1, {"b": "y"}], "c": null});
        let upper = map_strings_infallible(&value, &mut |text| Value::String(text.to_uppercase()));
        assert_eq!(
            upper,
            serde_json::json!({"a": ["X", 1, {"b": "Y"}], "c": null})
        );
        let mut seen = 0;
        let failed = map_strings(&value, &mut |text| {
            seen += 1;
            Err::<Value, String>(text.to_owned())
        });
        assert_eq!(failed, Err("x".to_owned()));
        assert_eq!(seen, 1);
    }
    use crate::flows::template_scope::render_in_scope;
    use serde_json::json;

    fn sample() -> FlowVariables {
        let mut vars = FlowVariables::default();
        vars.set_intrinsic(IntrinsicVariable::FileExt, json!("mkv"));
        vars.set_intrinsic(IntrinsicVariable::FileSize, json!(1234));
        vars.set_custom("codec", json!("hevc")).ok();
        vars
    }

    #[test]
    fn renders_tokens_defaults_and_unknowns() {
        let vars = sample();
        assert_eq!(
            render_in_scope(&json!("a.{$file.ext}"), &vars),
            json!("a.mkv")
        );
        assert_eq!(
            render_in_scope(&json!("{$vars.codec}-x"), &vars),
            json!("hevc-x")
        );
        assert_eq!(
            render_in_scope(&json!("{$vars.absent|h264}"), &vars),
            json!("h264")
        );
        assert_eq!(
            render_in_scope(&json!("[{$vars.absent}]"), &vars),
            json!("[]")
        );
    }

    #[test]
    fn escapes_and_keeps_unclosed_tokens() {
        let vars = sample();
        assert_eq!(
            render_in_scope(&json!("{{$file.ext}"), &vars),
            json!("{$file.ext}")
        );
        assert_eq!(
            render_in_scope(&json!("x {$file.ext"), &vars),
            json!("x {$file.ext")
        );
    }

    #[test]
    fn exact_token_keeps_json_type_and_recurses() {
        let vars = sample();
        let input = json!({
            "size": "{$file.size}",
            "list": ["{$vars.codec}", 3],
            "text": "n={$file.size}"
        });
        let rendered = render_in_scope(&input, &vars);
        assert_eq!(
            rendered,
            json!({"size": 1234, "list": ["hevc", 3], "text": "n=1234"})
        );
        assert_eq!(
            render_in_scope(&json!("{$vars.absent|d}"), &vars),
            json!("d")
        );
    }

    #[test]
    fn lists_template_tokens() {
        assert_eq!(
            template_tokens("{$file.stem}-{$vars.x|y}{{$z}"),
            vec!["file.stem".to_owned(), "vars.x".to_owned()]
        );
        assert!(is_known_expression("file.stem"));
        assert!(is_known_expression("vars.x"));
        assert!(is_known_expression("setting.url"));
        assert!(!is_known_expression("file.inconnu"));
    }

    #[test]
    fn validates_custom_variables() {
        let mut vars = FlowVariables::default();
        assert!(is_valid_variable_name("codec_cible-2"));
        assert!(!is_valid_variable_name("a.b"));
        assert!(!is_valid_variable_name(""));
        assert_eq!(
            vars.set_custom("file.name", json!("x")),
            Err(VariableError::Reserved("file.name".into()))
        );
        assert_eq!(
            vars.set_custom("bad name", json!("x")),
            Err(VariableError::InvalidName("bad name".into()))
        );
        assert_eq!(
            vars.set_custom("obj", json!({"a": 1})),
            Err(VariableError::NotScalar("obj".into()))
        );
        assert_eq!(
            vars.set_custom("long", json!("x".repeat(MAX_VARIABLE_VALUE_BYTES))),
            Err(VariableError::ValueTooLarge("long".into()))
        );
        for index in 0..MAX_CUSTOM_VARIABLES {
            assert!(vars.set_custom(&format!("v{index}"), json!(index)).is_ok());
        }
        assert_eq!(
            vars.set_custom("encore", json!(1)),
            Err(VariableError::TooMany)
        );
        assert!(vars.set_custom("v0", json!("remplacée")).is_ok());
    }

    #[test]
    fn merge_is_atomic() {
        let mut vars = sample();
        let mut incoming = Map::new();
        incoming.insert("ok".into(), json!(1));
        incoming.insert("mauvais nom".into(), json!(2));
        assert!(vars.merge_custom(&incoming).is_err());
        assert!(vars.lookup("vars.ok").is_none());
    }

    #[test]
    fn looks_up_and_builds_document() {
        let vars = sample();
        assert_eq!(vars.lookup("vars.codec"), Some(&json!("hevc")));
        assert_eq!(vars.lookup("file.ext"), Some(&json!("mkv")));
        assert_eq!(vars.lookup("file.inconnu"), None);
        let doc = vars.as_document();
        assert_eq!(doc["file"]["ext"], "mkv");
        assert_eq!(doc["vars"]["codec"], "hevc");
        assert!(FlowVariables::is_variable_property("vars.codec"));
        assert!(FlowVariables::is_variable_property("file.ext"));
        assert!(!FlowVariables::is_variable_property("format.duration"));
        assert!(!FlowVariables::is_variable_property("filename"));
    }

    #[test]
    fn intrinsic_keys_are_stable_through_serde() -> Result<(), serde_json::Error> {
        for var in IntrinsicVariable::ALL {
            let encoded = serde_json::to_value(var)?;
            assert_eq!(encoded, json!(var.key()));
            assert_eq!(serde_json::from_value::<IntrinsicVariable>(encoded)?, var);
            assert_eq!(IntrinsicVariable::from_key(var.key()), Some(var));
        }
        Ok(())
    }
}
