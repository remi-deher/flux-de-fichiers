//! Portée unique des modèles `{$expr|défaut}` (ADR 0026, « Modèle unique »).
//!
//! Tous les blocs (binaire, script, HTTP, sous-flux) résolvent leurs modèles par la même portée :
//! variables intrinsèques (`file.*`, `source.*`, `work.dir`…), variables personnalisées (`vars.*`),
//! métadonnées du fichier (`media.<chemin>`), sorties des nœuds déjà exécutés
//! (`nodes.<id>.message|nextPort|file`) et variables exposées par les plugins (`<espace>.<nom>`).

use super::variables::{
    CUSTOM_NAMESPACE, CUSTOM_PREFIX, FlowVariables, HTTP_NAMESPACES, IntrinsicVariable,
    TemplateToken, exact_token, is_valid_variable_name, map_strings, map_strings_infallible,
    render_piece, render_template_with,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Espace de noms des métadonnées du fichier : `{$media.video.codec}`.
pub const MEDIA_NAMESPACE: &str = "media";
/// Espace de noms des sorties de nœuds : `{$nodes.<id>.message}`.
pub const NODES_NAMESPACE: &str = "nodes";
/// Séparateur des segments d'une expression.
pub const PATH_SEPARATOR: char = '.';

/// Famille d'une expression de modèle, utilisée pour la résolution, la validation et les suggestions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeNamespace {
    /// Variable intrinsèque renseignée par le moteur (`file.path`, `job.id`…).
    Intrinsic,
    /// Variable personnalisée `vars.<nom>`.
    Custom,
    /// Champ des métadonnées du fichier `media.<chemin>`.
    Media,
    /// Sortie d'un nœud déjà exécuté `nodes.<id>.<champ>`.
    Node,
    /// Variable exposée par un plugin `<espace>.<nom>`.
    Plugin,
    /// Jeton résolu par l'exécuteur HTTP lui-même (`context.*`, `setting.*`…).
    Http,
}

impl ScopeNamespace {
    /// Famille d'une expression connue du moteur sans dépendre des plugins chargés.
    pub fn builtin(expr: &str) -> Option<Self> {
        let expr = expr.trim();
        if let Some(name) = expr.strip_prefix(CUSTOM_PREFIX) {
            return is_valid_variable_name(name).then_some(Self::Custom);
        }
        if IntrinsicVariable::from_key(expr).is_some() {
            return Some(Self::Intrinsic);
        }
        if media_path(expr).is_some() {
            return Some(Self::Media);
        }
        if split_node_reference(expr).is_some() {
            return Some(Self::Node);
        }
        HTTP_NAMESPACES
            .iter()
            .any(|namespace| expr.starts_with(namespace))
            .then_some(Self::Http)
    }
}

/// Champ qu'un nœud exécuté expose aux suivants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeOutputField {
    #[serde(rename = "message")]
    Message,
    #[serde(rename = "nextPort")]
    NextPort,
    #[serde(rename = "file")]
    File,
}

impl NodeOutputField {
    pub const ALL: [Self; 3] = [Self::Message, Self::NextPort, Self::File];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::NextPort => "nextPort",
            Self::File => "file",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::NextPort => "port de sortie",
            Self::File => "fichier courant",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.key() == key)
    }
}

/// Expression `nodes.<id>.<champ>` d'un nœud.
pub fn node_reference(node_id: &str, field: NodeOutputField) -> String {
    format!(
        "{NODES_NAMESPACE}{PATH_SEPARATOR}{node_id}{PATH_SEPARATOR}{}",
        field.key()
    )
}

/// Sépare `nodes.<id>.<champ>` : le champ est le dernier segment, l'identifiant tout ce qui
/// précède, points compris (un nœud peut s'appeler `encodage.v2`).
pub fn split_node_reference(expr: &str) -> Option<(&str, NodeOutputField)> {
    let (node_id, field) = expr
        .strip_prefix(NODES_NAMESPACE)?
        .strip_prefix(PATH_SEPARATOR)?
        .rsplit_once(PATH_SEPARATOR)?;
    let field = NodeOutputField::from_key(field)?;
    (!node_id.is_empty()).then_some((node_id, field))
}

/// Chemin dans les métadonnées d'une expression `media.<chemin>`.
pub fn media_path(expr: &str) -> Option<&str> {
    expr.strip_prefix(MEDIA_NAMESPACE)?
        .strip_prefix(PATH_SEPARATOR)
        .filter(|path| !path.is_empty())
}

/// Vrai si un plugin ne peut pas exposer ses variables sous cet espace de noms : il appartient au
/// moteur (intrinsèques, `vars`, `media`, `nodes`) ou aux exécuteurs HTTP.
pub fn is_reserved_namespace(namespace: &str) -> bool {
    namespace == CUSTOM_NAMESPACE
        || namespace == MEDIA_NAMESPACE
        || namespace == NODES_NAMESPACE
        || IntrinsicVariable::ALL
            .into_iter()
            .any(|var| var.namespace() == namespace)
        || HTTP_NAMESPACES
            .iter()
            .any(|prefix| prefix.trim_end_matches(PATH_SEPARATOR) == namespace)
}

/// Valeurs accessibles à un modèle. Une valeur `null` compte comme absente.
pub trait TemplateScope {
    /// Valeur de l'expression, si elle en a une à cet endroit.
    fn resolve(&self, expr: &str) -> Option<Value>;
    /// Famille de l'expression quand la portée la connaît, même sans valeur.
    fn namespace_of(&self, expr: &str) -> Option<ScopeNamespace>;
}

impl TemplateScope for FlowVariables {
    fn resolve(&self, expr: &str) -> Option<Value> {
        self.lookup(expr).filter(|value| !value.is_null()).cloned()
    }

    fn namespace_of(&self, expr: &str) -> Option<ScopeNamespace> {
        ScopeNamespace::builtin(expr).filter(|namespace| {
            matches!(
                namespace,
                ScopeNamespace::Intrinsic | ScopeNamespace::Custom | ScopeNamespace::Http
            )
        })
    }
}

/// Règle de rendu : souple (aperçu, configurations) ou stricte (variables déclarées d'un script).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderMode {
    /// Un texte vide retombe sur le défaut ; une expression inconnue donne un texte vide.
    Lenient,
    /// Comme l'ancien moteur : un texte vide est une valeur, et toute expression sans valeur ni
    /// défaut, connue ou non de la portée, est signalée manquante.
    Strict,
}

impl RenderMode {
    /// Valeur utilisable d'un jeton : jamais `null` ; un texte vide seulement en mode strict.
    fn present(self, value: Option<Value>) -> Option<Value> {
        value.filter(|value| {
            !value.is_null() && (self == Self::Strict || value.as_str() != Some(""))
        })
    }

    /// Vrai si une expression sans valeur ni défaut doit être signalée.
    fn reports(self, scope: &dyn TemplateScope, expr: &str) -> bool {
        self == Self::Strict || scope.namespace_of(expr).is_some()
    }
}

/// Rend une chaîne ; `missing` reçoit chaque expression sans valeur ni défaut que `mode` signale.
fn render_text(
    text: &str,
    scope: &dyn TemplateScope,
    mode: RenderMode,
    keep: &dyn Fn(&TemplateToken<'_>) -> bool,
    missing: &mut dyn FnMut(&str),
) -> Value {
    if let Some(expr) = exact_token(text)
        && let Some(value) = mode.present(scope.resolve(expr))
    {
        return value;
    }
    Value::String(render_template_with(text, &mut |token: &TemplateToken<
        '_,
    >| {
        if keep(token) {
            return verbatim_token(token);
        }
        let value = mode.present(scope.resolve(token.expr));
        if value.is_none() && token.default.is_empty() && mode.reports(scope, token.expr) {
            missing(token.expr);
        }
        render_piece(value.as_ref(), token.default)
    }))
}

/// Jeton à insérer dans un modèle pour une expression : `{$expr}`.
pub fn template_token(expr: &str) -> String {
    use super::variables::{TEMPLATE_CLOSE, TEMPLATE_OPEN};
    format!("{TEMPLATE_OPEN}{expr}{TEMPLATE_CLOSE}")
}

/// Réécrit un jeton tel qu'il a été saisi (`{$expr|défaut}`).
pub fn verbatim_token(token: &TemplateToken<'_>) -> String {
    use super::variables::DEFAULT_SEPARATOR;
    if token.default.is_empty() {
        template_token(token.expr)
    } else {
        template_token(&format!(
            "{}{DEFAULT_SEPARATOR}{}",
            token.expr, token.default
        ))
    }
}

/// Rend récursivement un JSON : une chaîne réduite à un seul jeton garde le type de sa valeur,
/// un jeton sans valeur donne son défaut ou un texte vide.
pub fn render_in_scope(value: &Value, scope: &dyn TemplateScope) -> Value {
    map_strings_infallible(value, &mut |text| {
        render_text(text, scope, RenderMode::Lenient, &|_| false, &mut |_| {})
    })
}

/// Comme [`render_in_scope`], mais les jetons que la portée ne connaît pas et ceux des
/// exécuteurs HTTP restent tels quels, pour que l'exécuteur HTTP les résolve lui-même.
pub fn render_in_scope_keeping_runner_tokens(value: &Value, scope: &dyn TemplateScope) -> Value {
    let keep = |token: &TemplateToken<'_>| {
        matches!(
            scope.namespace_of(token.expr),
            None | Some(ScopeNamespace::Http)
        )
    };
    map_strings_infallible(value, &mut |text| {
        if exact_token(text).is_some_and(|expr| {
            keep(&TemplateToken {
                expr,
                default: "",
                trailing: "",
            })
        }) {
            return Value::String(text.to_owned());
        }
        render_text(text, scope, RenderMode::Lenient, &keep, &mut |_| {})
    })
}

/// Rendu strict des variables déclarées d'un script : `Err` porte la première expression sans
/// valeur ni défaut, même inconnue de la portée (espace de noms d'un plugin non requis…). Un texte
/// vide compte comme une valeur, comme dans l'ancien moteur `{{…}}`.
pub fn render_in_scope_strict(value: &Value, scope: &dyn TemplateScope) -> Result<Value, String> {
    map_strings(value, &mut |text| {
        let mut first_missing: Option<String> = None;
        let rendered = render_text(text, scope, RenderMode::Strict, &|_| false, &mut |expr| {
            first_missing.get_or_insert_with(|| expr.to_owned());
        });
        first_missing.map_or(Ok(rendered), Err)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vars() -> Result<FlowVariables, super::super::VariableError> {
        let mut vars = FlowVariables::default();
        vars.set_intrinsic(IntrinsicVariable::FilePath, json!("/travail/a.mkv"));
        vars.set_intrinsic(IntrinsicVariable::FileSize, json!(42));
        vars.set_intrinsic(IntrinsicVariable::WorkerName, Value::Null);
        vars.set_custom("vide", json!(""))?;
        Ok(vars)
    }

    #[test]
    fn builtin_namespaces_are_classified() {
        assert_eq!(
            ScopeNamespace::builtin("file.path"),
            Some(ScopeNamespace::Intrinsic)
        );
        assert_eq!(
            ScopeNamespace::builtin("work.dir"),
            Some(ScopeNamespace::Intrinsic)
        );
        assert_eq!(
            ScopeNamespace::builtin("vars.x"),
            Some(ScopeNamespace::Custom)
        );
        assert_eq!(ScopeNamespace::builtin("vars.a.b"), None);
        assert_eq!(
            ScopeNamespace::builtin("media.video.codec"),
            Some(ScopeNamespace::Media)
        );
        assert_eq!(ScopeNamespace::builtin("media"), None);
        assert_eq!(
            ScopeNamespace::builtin("nodes.a.b.file"),
            Some(ScopeNamespace::Node)
        );
        assert_eq!(ScopeNamespace::builtin("nodes.a.inconnu"), None);
        assert_eq!(
            ScopeNamespace::builtin("setting.url"),
            Some(ScopeNamespace::Http)
        );
        assert_eq!(ScopeNamespace::builtin("mediainfo.video.codec"), None);
    }

    #[test]
    fn node_references_split_on_the_last_segment() {
        assert_eq!(
            split_node_reference("nodes.encodage.v2.file"),
            Some(("encodage.v2", NodeOutputField::File))
        );
        assert_eq!(split_node_reference("nodes..message"), None);
        assert_eq!(
            node_reference("a", NodeOutputField::NextPort),
            "nodes.a.nextPort"
        );
    }

    #[test]
    fn a_token_wraps_its_expression_in_template_delimiters() {
        assert_eq!(template_token("file.path"), "{$file.path}");
        assert_eq!(template_token("vars.x|d"), "{$vars.x|d}");
    }

    #[test]
    fn reserved_namespaces_are_derived_from_the_engine() {
        for namespace in [
            "nodes", "media", "vars", "file", "source", "work", "library", "worker", "job", "flow",
            "error", "context", "var", "setting", "config",
        ] {
            assert!(is_reserved_namespace(namespace), "{namespace}");
        }
        assert!(!is_reserved_namespace("mediainfo"));
    }

    #[test]
    fn single_token_keeps_its_type_and_null_or_empty_use_the_default()
    -> Result<(), super::super::VariableError> {
        let vars = vars()?;
        let rendered = render_in_scope(
            &json!({
                "size": "{$file.size}",
                "worker": "[{$worker.name}]",
                "vide": "{$vars.vide|défaut}",
                "absent": "{$vars.absent|d}"
            }),
            &vars,
        );
        assert_eq!(
            rendered,
            json!({"size": 42, "worker": "[]", "vide": "défaut", "absent": "d"})
        );
        Ok(())
    }

    #[test]
    fn strict_rendering_reports_known_expressions_without_value()
    -> Result<(), super::super::VariableError> {
        let vars = vars()?;
        assert_eq!(
            render_in_scope_strict(&json!("{$worker.name}"), &vars),
            Err("worker.name".to_owned())
        );
        assert_eq!(
            render_in_scope_strict(&json!("{$worker.name|w}"), &vars),
            Ok(json!("w"))
        );
        assert_eq!(
            render_in_scope_strict(&json!("x{$inconnu.y}"), &vars),
            Err("inconnu.y".to_owned())
        );
        assert_eq!(
            render_in_scope_strict(&json!("x{$inconnu.y|d}"), &vars),
            Ok(json!("xd"))
        );
        assert_eq!(
            render_in_scope_strict(&json!("{$file.path}"), &vars),
            Ok(json!("/travail/a.mkv"))
        );
        Ok(())
    }

    #[test]
    fn strict_rendering_keeps_empty_text_as_a_value() -> Result<(), super::super::VariableError> {
        let vars = vars()?;
        assert_eq!(
            render_in_scope_strict(&json!("{$vars.vide}"), &vars),
            Ok(json!(""))
        );
        assert_eq!(
            render_in_scope_strict(&json!("[{$vars.vide|d}]"), &vars),
            Ok(json!("[]"))
        );
        Ok(())
    }

    #[test]
    fn runner_tokens_and_unknown_expressions_stay_verbatim()
    -> Result<(), super::super::VariableError> {
        let vars = vars()?;
        let rendered = render_in_scope_keeping_runner_tokens(
            &json!({
                "a": "{$context.jobId}",
                "b": "{$file.path} {$setting.topic|t}",
                "c": "{$plugin.x}",
                "d": "{$file.size}"
            }),
            &vars,
        );
        assert_eq!(
            rendered,
            json!({
                "a": "{$context.jobId}",
                "b": "/travail/a.mkv {$setting.topic|t}",
                "c": "{$plugin.x}",
                "d": 42
            })
        );
        Ok(())
    }
}
