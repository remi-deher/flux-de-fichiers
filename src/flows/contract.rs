//! Contrats d'exécution déclaratifs d'opération chargés depuis les dépôts ou reçus dans un `Claim`.

use crate::{
    flows::custom_args::{CustomArgsError, safe_custom_args},
    media::{
        DurationRule, MagicBytesRule, OutputSpec, PostValidateMode, PostValidateSpec, ProbeFormat,
    },
    plugins::{CatalogItemType, catalog_with_roots, flow_block_catalog, load_manifest},
    tool_run::{ProgressParser, ProgressStream},
    tools_cache::{ToolCacheManager, host_arch},
};
use forgearr_shared::{
    exposed_variables::{
        ExposedVariables, ScriptRequirement, validate_exposed, validate_requirements,
    },
    flows::{FlowGraph, FlowNode, FlowRecord},
    jobs::JobErrorCode,
    script_variables::validate_variables,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use thiserror::Error;

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 15;
const DEFAULT_CONSOLE_TIMEOUT_SECS: u64 = 3600;
const DEFAULT_SCRIPT_TIMEOUT_SECS: u64 = 300;
const MAX_INLINE_SCRIPT_BYTES: usize = 512 * 1024;

fn default_console_timeout() -> u64 {
    DEFAULT_CONSOLE_TIMEOUT_SECS
}

fn default_http_timeout() -> u64 {
    DEFAULT_HTTP_TIMEOUT_SECS
}

fn default_script_timeout() -> u64 {
    DEFAULT_SCRIPT_TIMEOUT_SECS
}

/// Erreur d'évaluation ou de résolution d'un contrat d'opération.
#[derive(Debug, Error)]
pub enum ContractError {
    #[error("opération introuvable pour le bloc « {0} »")]
    UnknownOperation(String),
    #[error("paramètres personnalisés invalides : {0}")]
    InvalidCustomArgs(#[from] CustomArgsError),
    #[error("chemin d'exécution manquant dans le contrat ({0})")]
    MissingPathParam(String),
    #[error("caractère de contrôle interdit dans le paramètre « {0} »")]
    InvalidScalarParam(String),
    #[error("contrat JSON invalide : {0}")]
    InvalidContract(String),
}

/// Refuse un contrat dont les variables, expositions ou dépendances déclarées sont invalides.
fn validate_contract_declarations(contract: &OperationContract) -> Result<(), ContractError> {
    let invalid =
        |reason: String| ContractError::InvalidContract(format!("{} : {reason}", contract.id));
    if let Some(exposed) = &contract.exposes {
        validate_exposed(exposed).map_err(|err| invalid(err.to_string()))?;
    }
    if let OperationRuntime::Script(script) = &contract.runtime {
        validate_variables(&script.variables).map_err(|err| invalid(err.to_string()))?;
        validate_requirements(&script.requires).map_err(|err| invalid(err.to_string()))?;
    }
    Ok(())
}

/// Référence déclarative vers un outil du catalogue requis par une opération.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRef {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    "1.0.0".to_owned()
}

/// Contrat complet d'une opération (`plugins/*/operations/*.json`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationContract {
    pub schema_version: String,
    pub id: String,
    pub version: String,
    pub name: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub capability: String,
    pub status: String,
    pub runtime: OperationRuntime,
    /// Variables que l'opération expose aux scripts via `mediaInfo` (ADR 0025).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposes: Option<ExposedVariables>,
}

/// Définition d'exécution selon la nature (`primitive`, `binary`, `http`, `script`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OperationRuntime {
    Primitive(PrimitiveRuntime),
    Binary(BinaryRuntime),
    Http(HttpRuntime),
    Script(ScriptRuntime),
}

pub use forgearr_shared::{flows::scalar_to_string, local::ScriptInterpreter};

/// Contrat d'exécution d'un script ou exécutable agnostique via `stdin`/`stdout` JSON (`kind: "script"`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptRuntime {
    #[serde(default)]
    pub tool: Option<ToolRef>,
    /// Outils du cache placés en tête du `PATH` de la commande (ex. `ffmpeg` appelé par l'outil).
    #[serde(default)]
    pub path_tools: Vec<ToolRef>,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub interpreter: Option<ScriptInterpreter>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub error_code: Option<JobErrorCode>,
    #[serde(default = "default_script_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub defaults: Map<String, Value>,
    /// Variables typées du script (ADR 0025) ; vide pour les scripts sans schéma.
    #[serde(default)]
    pub variables: Vec<forgearr_shared::script_variables::ScriptVariable>,
    /// Variables exposées par des plugins que le script consomme ; dépendance dure par défaut.
    #[serde(default)]
    pub requires: Vec<ScriptRequirement>,
    /// Script intégré sans ses dépendances : elles ne sont pas contrôlées (saisie manuelle).
    #[serde(default)]
    pub dependencies_unverified: bool,
    /// Le script accepte une entrée non média (ex. archive à extraire) : un flux qui le contient
    /// tolère l'échec de la sonde initiale.
    #[serde(default)]
    pub accepts_non_media: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveKind {
    Entry,
    Terminal,
    Condition,
    /// Aiguille selon les capacités du worker courant (ADR 0018).
    Capability,
    /// Relaie la tâche vers la file d'un autre worker (ADR 0018).
    Relay,
    /// Écrit une variable personnalisée `vars.<nom>` dans le contexte de la tâche.
    #[serde(rename = "set-variable")]
    SetVariable,
    /// Exécute un autre flux puis reprend à la sortie du bloc (ADR 0026).
    #[serde(rename = "sub-flow")]
    SubFlow,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveRuntime {
    pub primitive: PrimitiveKind,
    #[serde(default)]
    pub defaults: Map<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryExecutionMode {
    StagedOutput,
    InPlace,
    ProbePrimary,
    ProbeEnrich,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleConfig {
    #[serde(default)]
    pub progress_format: ProgressParser,
    #[serde(default)]
    pub progress_stream: ProgressStream,
    #[serde(default)]
    pub success_exit_codes: Vec<i32>,
    #[serde(default)]
    pub warning_exit_codes: Vec<i32>,
    #[serde(default = "default_console_timeout")]
    pub timeout_seconds: u64,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            progress_format: ProgressParser::None,
            progress_stream: ProgressStream::Stderr,
            success_exit_codes: vec![0],
            warning_exit_codes: Vec::new(),
            timeout_seconds: DEFAULT_CONSOLE_TIMEOUT_SECS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedRule {
    pub target: String,
    pub kind: String,
    pub inputs: Vec<String>,
    #[serde(default)]
    pub table: HashMap<String, Value>,
    #[serde(default)]
    pub default: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamFormat {
    FilePath,
    SafeCustomArgs,
    String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgItem {
    Literal(String),
    Conditional {
        when: ConditionClause,
        then: Vec<ArgItem>,
        #[serde(default, rename = "else")]
        otherwise: Vec<ArgItem>,
    },
    Format {
        format: String,
    },
    Param {
        param: String,
        #[serde(default, rename = "as")]
        format: Option<ParamFormat>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ConditionClause {
    All {
        all: Vec<ConditionClause>,
    },
    Any {
        any: Vec<ConditionClause>,
    },
    Predicate {
        param: String,
        #[serde(default)]
        eq: Option<Value>,
        #[serde(default)]
        neq: Option<Value>,
        #[serde(default, rename = "in")]
        in_list: Option<Vec<Value>>,
        #[serde(default, rename = "endsWith")]
        ends_with: Option<String>,
        #[serde(default, rename = "startsWith")]
        starts_with: Option<String>,
        #[serde(default)]
        truthy: Option<bool>,
        #[serde(default, rename = "nonEmpty")]
        non_empty: Option<bool>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryFallbackRule {
    pub when: ConditionClause,
    #[serde(default)]
    pub overrides: Map<String, Value>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeMatch {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub priority: i32,
}

impl ProbeMatch {
    pub fn is_wildcard(&self) -> bool {
        self.extensions.is_empty() || self.extensions.iter().any(|ext| ext.trim() == "*")
    }

    pub fn matches_path(&self, path: &Path) -> bool {
        if self.is_wildcard() {
            return true;
        }
        let Some(file_ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        self.extensions.iter().any(|ext| {
            ext.trim()
                .trim_start_matches('.')
                .eq_ignore_ascii_case(file_ext)
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryRuntime {
    #[serde(default)]
    pub default_fallback: bool,
    #[serde(default)]
    pub tool: Option<ToolRef>,
    /// Outils du cache placés en tête du `PATH` de la commande (ex. `ffmpeg` appelé par l'outil).
    #[serde(default)]
    pub path_tools: Vec<ToolRef>,
    pub executable: String,
    #[serde(default)]
    pub env_override: Option<String>,
    pub execution_mode: BinaryExecutionMode,
    #[serde(default)]
    pub probe_format: Option<ProbeFormat>,
    #[serde(default)]
    pub probe_require_paths: Vec<String>,
    #[serde(default)]
    pub probe_enrichment_key: Option<String>,
    #[serde(default)]
    pub probe_match: ProbeMatch,
    #[serde(default)]
    pub output_extension: Option<String>,
    #[serde(default)]
    pub output_content_type: Option<String>,
    #[serde(default)]
    pub output_filename: Option<String>,
    #[serde(default)]
    pub post_validate: PostValidateMode,
    #[serde(default)]
    pub post_validate_magic: Option<MagicBytesRule>,
    #[serde(default)]
    pub post_validate_preserve_paths: Vec<String>,
    #[serde(default)]
    pub duration_rules: Vec<DurationRule>,
    #[serde(default)]
    pub skip_when_unchanged: bool,
    #[serde(default)]
    pub error_code: Option<JobErrorCode>,
    #[serde(default)]
    pub success_message: Option<String>,
    #[serde(default)]
    pub warning_message: Option<String>,
    #[serde(default)]
    pub defaults: Map<String, Value>,
    #[serde(default)]
    pub custom_args_forbidden: Vec<String>,
    #[serde(default)]
    pub derived: Vec<DerivedRule>,
    #[serde(default)]
    pub argv: Vec<ArgItem>,
    #[serde(default)]
    pub retry_fallback: Vec<RetryFallbackRule>,
    #[serde(default)]
    pub console: ConsoleConfig,
}

impl BinaryRuntime {
    pub fn post_validate_spec(&self) -> PostValidateSpec {
        PostValidateSpec {
            mode: self.post_validate,
            magic: self.post_validate_magic.clone(),
            preserve_paths: self.post_validate_preserve_paths.clone(),
        }
    }

    pub fn output_spec(&self) -> Option<OutputSpec> {
        let has_output = self.output_extension.is_some()
            || self.output_content_type.is_some()
            || self.post_validate != PostValidateMode::None;
        if !has_output {
            return None;
        }
        Some(OutputSpec {
            extension: self
                .output_extension
                .clone()
                .unwrap_or_else(|| "bin".to_owned()),
            content_type: self
                .output_content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_owned()),
            post_validate: self.post_validate_spec(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathMappingContract {
    pub local_key: String,
    pub remote_key: String,
    pub output_var: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpExtractKind {
    ArrayPathPrefix,
    ArrayRelativePath,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpExtractRule {
    pub kind: HttpExtractKind,
    pub path_field: String,
    #[serde(default)]
    pub absolute_path_field: Option<String>,
    #[serde(default)]
    pub base_path_var: Option<String>,
    pub id_field: String,
    pub target_var: String,
    pub save_as: String,
    #[serde(default)]
    pub save_path_as: Option<String>,
    #[serde(default)]
    pub skip_message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpStep {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub query: Option<Map<String, Value>>,
    #[serde(default)]
    pub headers: Option<Map<String, Value>>,
    #[serde(default)]
    pub merge_custom_headers: bool,
    #[serde(default)]
    pub body: Option<Value>,
    #[serde(default)]
    pub extract: Option<HttpExtractRule>,
    #[serde(default)]
    pub success_message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRuntime {
    #[serde(default)]
    pub settings_plugin_id: Option<String>,
    #[serde(default)]
    pub required_settings: Vec<String>,
    #[serde(default)]
    pub error_code: Option<JobErrorCode>,
    #[serde(default)]
    pub fail_on_error_param: Option<String>,
    #[serde(default = "default_http_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub path_mapping: Option<PathMappingContract>,
    #[serde(default)]
    pub defaults: Map<String, Value>,
    #[serde(default)]
    pub steps: Vec<HttpStep>,
}

/// Méthodes HTTP en lecture seule.
const SAFE_HTTP_METHODS: [&str; 3] = ["GET", "HEAD", "OPTIONS"];

impl HttpRuntime {
    /// Toutes les étapes sont-elles en lecture seule ? Une méthode calculée ne l'est jamais.
    pub fn is_safe(&self) -> bool {
        self.steps.iter().all(|step| {
            let method = step.method.trim();
            SAFE_HTTP_METHODS
                .iter()
                .any(|safe| method.eq_ignore_ascii_case(safe))
        })
    }
}

/// Charge le fichier `entrypoint` d'une opération `kind: "script"` depuis le dossier du manifeste
/// afin de le transporter dans `Claim` vers les workers distants, sans jamais sortir de `manifest_dir`.
fn resolve_script_entrypoint(manifest_dir: &Path, raw: &mut Value) {
    let Some(runtime) = raw.get_mut("runtime").and_then(Value::as_object_mut) else {
        return;
    };
    if runtime.get("kind").and_then(Value::as_str) != Some("script") {
        return;
    }
    if runtime
        .get("code")
        .and_then(Value::as_str)
        .is_some_and(|code| !code.is_empty())
    {
        return;
    }
    let Some(entrypoint) = runtime.get("entrypoint").and_then(Value::as_str) else {
        return;
    };
    let rel = Path::new(entrypoint);
    if !ToolCacheManager::is_safe_relative_path(rel) {
        return;
    }
    let Ok(base) = std::fs::canonicalize(manifest_dir) else {
        return;
    };
    let Ok(resolved) = std::fs::canonicalize(base.join(rel)) else {
        return;
    };
    if !resolved.starts_with(&base) {
        return;
    }
    let Ok(bytes) = std::fs::read(&resolved) else {
        return;
    };
    if bytes.len() > MAX_INLINE_SCRIPT_BYTES {
        return;
    }
    if let Ok(code) = String::from_utf8(bytes) {
        runtime.insert("code".to_owned(), Value::String(code));
    }
}

/// Registre en mémoire des contrats d'opérations et de la table bloc/alias → `operationId`.
#[derive(Clone, Debug, Default)]
pub struct OperationRegistry {
    operations: HashMap<String, OperationContract>,
    raw_operations: HashMap<String, Value>,
    block_operations: HashMap<String, String>,
}

impl OperationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_raw(
        operations: HashMap<String, Value>,
        block_operations: HashMap<String, String>,
    ) -> Self {
        let mut registry = Self {
            operations: HashMap::new(),
            raw_operations: HashMap::new(),
            block_operations,
        };
        for (_, raw) in operations {
            let _ = registry.register_json(raw);
        }
        registry
    }

    pub fn load_from_roots(roots: &[PathBuf]) -> Self {
        let mut registry = Self::new();
        for (root, entry) in catalog_with_roots(roots) {
            if !matches!(
                entry.entry_type,
                CatalogItemType::Plugin | CatalogItemType::Script
            ) {
                continue;
            }
            let Ok(manifest) = load_manifest(&root, &entry.manifest_path) else {
                continue;
            };
            let manifest_rel = Path::new(&entry.manifest_path);
            let manifest_dir = root.join(manifest_rel.parent().unwrap_or_else(|| Path::new("")));
            for file in manifest.files {
                if file.role.as_deref() != Some("operation") {
                    continue;
                }
                let rel_path = Path::new(&file.path);
                if !ToolCacheManager::is_safe_relative_path(rel_path) {
                    continue;
                }
                let full_path = manifest_dir.join(rel_path);
                let Ok(text) = std::fs::read_to_string(&full_path) else {
                    continue;
                };
                if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
                    resolve_script_entrypoint(&manifest_dir, &mut value);
                    let _ = registry.register_json(value);
                }
            }
        }
        for spec in flow_block_catalog(roots) {
            if let Some(op_id) = spec.operation_id {
                registry.register_block_mapping(&spec.kind, &op_id);
                for alias in spec.aliases {
                    registry.register_block_mapping(&alias, &op_id);
                }
            }
        }
        registry
    }

    pub fn register_json(&mut self, raw: Value) -> Result<(), ContractError> {
        let contract: OperationContract = serde_json::from_value(raw.clone())
            .map_err(|err| ContractError::InvalidContract(err.to_string()))?;
        validate_contract_declarations(&contract)?;
        self.raw_operations.insert(contract.id.clone(), raw);
        self.operations.insert(contract.id.clone(), contract);
        Ok(())
    }

    /// Expositions des plugins, triées par opération ; un espace de noms déjà pris l'est par la première.
    pub fn exposed_variables(&self) -> Vec<&ExposedVariables> {
        self.exposed_variables_where(|_| true)
    }

    /// Expositions déclarées avec le plugin fournisseur de chacune, triées par identifiant d'opération.
    pub fn exposure_claims(&self) -> Vec<(&str, &ExposedVariables)> {
        let mut exposing: Vec<&OperationContract> = self
            .operations
            .values()
            .filter(|op| op.exposes.is_some())
            .collect();
        exposing.sort_by(|a, b| a.id.cmp(&b.id));
        exposing
            .into_iter()
            .filter_map(|op| {
                op.exposes
                    .as_ref()
                    .map(|exposed| (op.provider_id.as_str(), exposed))
            })
            .collect()
    }

    /// Expositions que désignent les dépendances d'un script : pour chacune, celle du plugin fournisseur
    /// qui fournit la variable dans la version majeure visée. Un espace de noms contesté entre plusieurs
    /// plugins se résout ainsi par la dépendance du script et non par l'ordre des opérations.
    pub fn exposures_bound_by(&self, requirements: &[ScriptRequirement]) -> Vec<&ExposedVariables> {
        let claims = self.exposure_claims();
        let mut bound: Vec<&ExposedVariables> = Vec::new();
        for requirement in requirements {
            let found = claims
                .iter()
                .find(|claim| claim.0 == requirement.provider && claim.1.provides(requirement))
                .map(|claim| claim.1);
            if let Some(exposed) = found
                && !bound.iter().any(|known| std::ptr::eq(*known, exposed))
            {
                bound.push(exposed);
            }
        }
        bound
    }

    /// Expositions lues par un script : celles de ses dépendances, puis la première exposition de chaque
    /// autre espace de noms (script sans `requires`, ou référence à un espace qu'il ne déclare pas).
    pub fn exposures_for_requirements(
        &self,
        requirements: &[ScriptRequirement],
    ) -> Vec<&ExposedVariables> {
        let mut chosen = self.exposures_bound_by(requirements);
        for fallback in self.exposed_variables() {
            if !chosen
                .iter()
                .any(|known| known.namespace == fallback.namespace)
            {
                chosen.push(fallback);
            }
        }
        chosen
    }

    /// Comme `exposed_variables`, limité aux opérations retenues par `keep`.
    pub fn exposed_variables_where(
        &self,
        keep: impl Fn(&OperationContract) -> bool,
    ) -> Vec<&ExposedVariables> {
        let mut exposing: Vec<&OperationContract> = self
            .operations
            .values()
            .filter(|op| op.exposes.is_some() && keep(op))
            .collect();
        exposing.sort_by(|a, b| a.id.cmp(&b.id));
        let mut seen = HashSet::new();
        exposing
            .into_iter()
            .filter_map(|op| op.exposes.as_ref())
            .filter(|exposed| seen.insert(exposed.namespace.as_str()))
            .collect()
    }

    /// Un plugin fournisseur chargé expose-t-il la variable, dans la version majeure visée ?
    pub fn is_requirement_met(&self, requirement: &ScriptRequirement) -> bool {
        self.operations.values().any(|op| {
            op.provider_id == requirement.provider
                && op
                    .exposes
                    .as_ref()
                    .is_some_and(|exposed| exposed.provides(requirement))
        })
    }

    /// Dépendances d'un script que ce registre ne satisfait pas.
    pub fn unmet_requirements<'a>(
        &self,
        requirements: &'a [ScriptRequirement],
    ) -> Vec<&'a ScriptRequirement> {
        requirements
            .iter()
            .filter(|requirement| !self.is_requirement_met(requirement))
            .collect()
    }

    pub fn register_block_mapping(&mut self, block_or_alias: &str, op_id: &str) {
        self.block_operations
            .insert(block_or_alias.to_owned(), op_id.to_owned());
    }

    pub fn get(&self, operation_id: &str) -> Option<&OperationContract> {
        self.operations.get(operation_id)
    }

    pub fn operation_id_for_kind(&self, kind: &str) -> Option<&str> {
        self.block_operations
            .get(kind)
            .map(String::as_str)
            .or_else(|| self.operations.get(kind).map(|op| op.id.as_str()))
    }

    pub fn operation_id_for_node(&self, node: &FlowNode) -> Option<&str> {
        if let Some(explicit) = node.extra.get("operationId").and_then(Value::as_str)
            && let Some((key, _)) = self.operations.get_key_value(explicit)
        {
            return Some(key.as_str());
        }
        self.operation_id_for_kind(node.kind.as_str())
    }

    pub fn resolve_node_operation(&self, node: &FlowNode) -> Option<&OperationContract> {
        let op_id = self.operation_id_for_node(node)?;
        self.operations.get(op_id)
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn primary_probe(&self) -> Option<&OperationContract> {
        let mut matches: Vec<(&OperationContract, &BinaryRuntime)> = self
            .operations
            .values()
            .filter_map(|op| match &op.runtime {
                OperationRuntime::Binary(bin)
                    if bin.execution_mode == BinaryExecutionMode::ProbePrimary =>
                {
                    Some((op, bin))
                }
                _ => None,
            })
            .collect();
        matches.sort_by(|(a_op, a_bin), (b_op, b_bin)| {
            b_bin
                .probe_match
                .priority
                .cmp(&a_bin.probe_match.priority)
                .then_with(|| a_op.id.cmp(&b_op.id))
        });
        matches.into_iter().map(|(op, _)| op).next()
    }

    pub fn primary_probe_for_file(&self, path: &Path) -> Option<&OperationContract> {
        let mut matches: Vec<(&OperationContract, &BinaryRuntime)> = self
            .operations
            .values()
            .filter_map(|op| match &op.runtime {
                OperationRuntime::Binary(bin)
                    if bin.execution_mode == BinaryExecutionMode::ProbePrimary
                        && bin.probe_match.matches_path(path) =>
                {
                    Some((op, bin))
                }
                _ => None,
            })
            .collect();
        matches.sort_by(|(a_op, a_bin), (b_op, b_bin)| {
            a_bin
                .probe_match
                .is_wildcard()
                .cmp(&b_bin.probe_match.is_wildcard())
                .then_with(|| b_bin.probe_match.priority.cmp(&a_bin.probe_match.priority))
                .then_with(|| a_op.id.cmp(&b_op.id))
        });
        matches.into_iter().map(|(op, _)| op).next()
    }

    pub fn enrich_probes(&self) -> Vec<&OperationContract> {
        let mut matches: Vec<(&OperationContract, &BinaryRuntime)> = self
            .operations
            .values()
            .filter_map(|op| match &op.runtime {
                OperationRuntime::Binary(bin)
                    if bin.execution_mode == BinaryExecutionMode::ProbeEnrich =>
                {
                    Some((op, bin))
                }
                _ => None,
            })
            .collect();
        matches.sort_by(|(a_op, a_bin), (b_op, b_bin)| {
            b_bin
                .probe_match
                .priority
                .cmp(&a_bin.probe_match.priority)
                .then_with(|| a_op.id.cmp(&b_op.id))
        });
        matches.into_iter().map(|(op, _)| op).collect()
    }

    pub fn enrich_probes_for_file(&self, path: &Path) -> Vec<&OperationContract> {
        self.enrich_probes()
            .into_iter()
            .filter(|op| {
                matches!(
                    &op.runtime,
                    OperationRuntime::Binary(bin) if bin.probe_match.matches_path(path)
                )
            })
            .collect()
    }

    /// L'opération est-elle une sonde, exécutée avant tout nœud du flux ?
    pub fn is_probe(op: &OperationContract) -> bool {
        matches!(
            &op.runtime,
            OperationRuntime::Binary(bin)
                if matches!(
                    bin.execution_mode,
                    BinaryExecutionMode::ProbePrimary | BinaryExecutionMode::ProbeEnrich
                )
        )
    }

    pub fn all_probes(&self) -> Vec<&OperationContract> {
        let mut matches: Vec<&OperationContract> = self
            .operations
            .values()
            .filter(|op| {
                matches!(
                    &op.runtime,
                    OperationRuntime::Binary(bin)
                        if matches!(
                            bin.execution_mode,
                            BinaryExecutionMode::ProbePrimary | BinaryExecutionMode::ProbeEnrich
                        )
                )
            })
            .collect();
        matches.sort_by(|a, b| a.id.cmp(&b.id));
        matches
    }

    pub fn default_fallback_operation(&self) -> Option<&OperationContract> {
        let mut matches: Vec<&OperationContract> = self
            .operations
            .values()
            .filter(|op| {
                matches!(
                    &op.runtime,
                    OperationRuntime::Binary(bin) if bin.default_fallback
                )
            })
            .collect();
        matches.sort_by(|a, b| a.id.cmp(&b.id));
        matches.into_iter().next()
    }

    pub fn required_tool_refs_for_flow(&self, flow: Option<&FlowRecord>) -> Vec<ToolRef> {
        let mut seen = HashSet::new();
        let mut tools = Vec::new();
        let mut push_op_tool = |op: &OperationContract| {
            let (tool, path_tools) = match &op.runtime {
                OperationRuntime::Binary(bin) => (bin.tool.as_ref(), bin.path_tools.as_slice()),
                OperationRuntime::Script(script) => {
                    (script.tool.as_ref(), script.path_tools.as_slice())
                }
                OperationRuntime::Primitive(_) | OperationRuntime::Http(_) => (None, &[][..]),
            };
            for tool in tool.into_iter().chain(path_tools) {
                if seen.insert(tool.id.clone()) {
                    tools.push(tool.clone());
                }
            }
        };

        for probe in self.all_probes() {
            push_op_tool(probe);
        }
        match flow.and_then(|f| serde_json::from_str::<FlowGraph>(&f.definition).ok()) {
            Some(graph) => {
                for node in &graph.nodes {
                    if let Some(op) = self.resolve_node_operation(node) {
                        push_op_tool(op);
                    }
                }
            }
            None => {
                if let Some(default_op) = self.default_fallback_operation() {
                    push_op_tool(default_op);
                }
            }
        }
        tools
    }

    /// Le graphe contient-il une opération qui accepte une entrée non média ?
    pub fn graph_accepts_non_media(&self, graph: &FlowGraph) -> bool {
        graph.nodes.iter().any(|node| {
            self.resolve_node_operation(node).is_some_and(|op| {
                matches!(&op.runtime, OperationRuntime::Script(script) if script.accepts_non_media)
            })
        })
    }

    pub fn required_settings_plugins_for_flow(&self, flow: Option<&FlowRecord>) -> Vec<String> {
        let Some(graph) = flow.and_then(|f| serde_json::from_str::<FlowGraph>(&f.definition).ok())
        else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut plugins = Vec::new();
        for node in &graph.nodes {
            if let Some(op) = self.resolve_node_operation(node)
                && let OperationRuntime::Http(http) = &op.runtime
                && let Some(plugin_id) = &http.settings_plugin_id
                && seen.insert(plugin_id.clone())
            {
                plugins.push(plugin_id.clone());
            }
        }
        plugins
    }

    pub fn claim_payload_for_flow(
        &self,
        flow: Option<&FlowRecord>,
    ) -> (HashMap<String, Value>, HashMap<String, String>) {
        let mut ops = HashMap::new();
        let mut blocks = HashMap::new();
        let mut insert_op = |op: &OperationContract| {
            if let Some(raw) = self.raw_operations.get(&op.id) {
                ops.insert(op.id.clone(), raw.clone());
            }
        };
        for probe in self.all_probes() {
            insert_op(probe);
        }
        if let Some(default_op) = self.default_fallback_operation() {
            insert_op(default_op);
        }
        if let Some(graph) =
            flow.and_then(|f| serde_json::from_str::<FlowGraph>(&f.definition).ok())
        {
            for node in &graph.nodes {
                if let Some(op) = self.resolve_node_operation(node) {
                    blocks.insert(node.kind.as_str().to_owned(), op.id.clone());
                    insert_op(op);
                }
                if let Some(script_id) = node.config.get("scriptId").and_then(Value::as_str)
                    && let Some(target_op) = self.get(&format!("operation.{script_id}"))
                {
                    insert_op(target_op);
                }
            }
        }
        (ops, blocks)
    }

    pub fn duration_rules(&self) -> Vec<DurationRule> {
        let mut rules = Vec::new();
        for op in self.all_probes() {
            if let OperationRuntime::Binary(bin) = &op.runtime {
                for rule in &bin.duration_rules {
                    if let Some(key) = &bin.probe_enrichment_key {
                        rules.push(DurationRule {
                            path: format!("{key}.{}", rule.path),
                            scale: rule.scale,
                        });
                    }
                    rules.push(rule.clone());
                }
            }
        }
        rules
    }

    pub fn output_spec_for_flow(
        &self,
        flow: Option<&FlowRecord>,
        requested_ext: Option<&str>,
    ) -> Option<OutputSpec> {
        let default_spec = self
            .default_fallback_operation()
            .and_then(|op| match &op.runtime {
                OperationRuntime::Binary(bin) => bin.output_spec(),
                _ => None,
            })
            .unwrap_or_default();

        let Some(flow) = flow else {
            return match requested_ext {
                None => Some(default_spec),
                Some(ext) if ext.eq_ignore_ascii_case(&default_spec.extension) => {
                    Some(default_spec)
                }
                _ => None,
            };
        };
        let Ok(graph) = serde_json::from_str::<FlowGraph>(&flow.definition) else {
            return Some(default_spec);
        };
        let mut specs: Vec<OutputSpec> = Vec::new();
        for node in &graph.nodes {
            if let Some(OperationContract {
                runtime: OperationRuntime::Binary(bin),
                ..
            }) = self.resolve_node_operation(node)
                && let Some(spec) = bin.output_spec()
            {
                specs.push(spec);
            }
        }

        if let Some(ext) = requested_ext {
            if let Some(matched) = specs
                .iter()
                .rev()
                .find(|spec| spec.extension.eq_ignore_ascii_case(ext))
            {
                return Some(matched.clone());
            }
            if specs.is_empty() && default_spec.extension.eq_ignore_ascii_case(ext) {
                return Some(default_spec);
            }
            return None;
        }
        specs.pop().or(Some(default_spec))
    }

    pub fn flow_readiness_warnings(
        &self,
        flow: &FlowRecord,
        cache_mgr: &ToolCacheManager,
        configured_plugins: &HashSet<String>,
    ) -> Vec<String> {
        let mut warnings = Vec::new();
        let arch = host_arch();
        for tool in self.required_tool_refs_for_flow(Some(flow)) {
            if !cache_mgr.is_ready(&tool.id, &tool.version, arch) {
                warnings.push(format!(
                    "outil manquant ({}@{}) — sera installé à la demande lors de l'exécution",
                    tool.id, tool.version
                ));
            }
        }
        for plugin_id in self.required_settings_plugins_for_flow(Some(flow)) {
            if !configured_plugins.contains(&plugin_id) {
                warnings.push(format!(
                    "réglages du plugin « {plugin_id} » incomplets ou absents"
                ));
            }
        }
        warnings
    }
}

/// Construit le vecteur `argv` résolu pour un contrat binaire et retourne `(argv, modified_flags_present)`.
pub fn build_binary_argv(
    runtime: &BinaryRuntime,
    node_config: &Value,
    paths: &HashMap<&str, &Path>,
    overrides: Option<&Map<String, Value>>,
) -> Result<(Vec<String>, bool), ContractError> {
    let mut params = merge_params(&runtime.defaults, node_config);
    if let Some(extra) = overrides {
        for (k, v) in extra {
            params.insert(k.clone(), v.clone());
        }
    }
    apply_derived(&mut params, &runtime.derived);

    let mut argv = Vec::new();
    let mut non_path_tokens = 0usize;
    expand_arg_items(
        &runtime.argv,
        runtime,
        &params,
        paths,
        &mut argv,
        &mut non_path_tokens,
    )?;
    Ok((argv, non_path_tokens > 0))
}

pub fn merge_params(defaults: &Map<String, Value>, config: &Value) -> Map<String, Value> {
    let mut merged = defaults.clone();
    if let Some(obj) = config.as_object() {
        for (key, value) in obj {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

pub fn apply_derived(params: &mut Map<String, Value>, rules: &[DerivedRule]) {
    for rule in rules {
        if rule.kind != "map" {
            continue;
        }
        let inputs: Vec<String> = rule
            .inputs
            .iter()
            .map(|name| scalar_to_string(params.get(name)))
            .collect();
        let exact_key = inputs.join("|");
        let resolved = rule
            .table
            .get(&exact_key)
            .cloned()
            .or_else(|| match_wildcard_table(&rule.table, &inputs))
            .unwrap_or_else(|| rule.default.clone());
        params.insert(rule.target.clone(), resolved);
    }
}

fn match_wildcard_table(table: &HashMap<String, Value>, inputs: &[String]) -> Option<Value> {
    let mut best: Option<(usize, &Value)> = None;
    for (pattern, value) in table {
        let parts: Vec<&str> = pattern.split('|').collect();
        if parts.len() != inputs.len() {
            continue;
        }
        let mut exact_count = 0usize;
        let mut matched = true;
        for (pat, input) in parts.iter().zip(inputs.iter()) {
            if *pat == "*" {
                continue;
            } else if *pat == input {
                exact_count += 1;
            } else {
                matched = false;
                break;
            }
        }
        if matched && best.is_none_or(|(best_score, _)| exact_count > best_score) {
            best = Some((exact_count, value));
        }
    }
    best.map(|(_, val)| val.clone())
}

fn expand_arg_items(
    items: &[ArgItem],
    runtime: &BinaryRuntime,
    params: &Map<String, Value>,
    paths: &HashMap<&str, &Path>,
    out: &mut Vec<String>,
    non_path_tokens: &mut usize,
) -> Result<(), ContractError> {
    for item in items {
        match item {
            ArgItem::Literal(lit) => {
                out.push(lit.clone());
                *non_path_tokens += 1;
            }
            ArgItem::Conditional {
                when,
                then,
                otherwise,
            } => {
                let branch = if evaluate_condition(when, params) {
                    then
                } else {
                    otherwise
                };
                expand_arg_items(branch, runtime, params, paths, out, non_path_tokens)?;
            }
            ArgItem::Format { format } => {
                let rendered = format_param_template(format, params)?;
                out.push(rendered);
                *non_path_tokens += 1;
            }
            ArgItem::Param { param, format } => match format {
                Some(ParamFormat::FilePath) => {
                    let path = paths
                        .get(param.as_str())
                        .ok_or_else(|| ContractError::MissingPathParam(param.clone()))?;
                    out.push(path.to_string_lossy().into_owned());
                }
                Some(ParamFormat::SafeCustomArgs) => {
                    let raw = scalar_to_string(params.get(param));
                    if !raw.trim().is_empty() {
                        let extra = safe_custom_args(&raw, &runtime.custom_args_forbidden)?;
                        *non_path_tokens += extra.len();
                        out.extend(extra);
                    }
                }
                Some(ParamFormat::String) | None => {
                    let val = scalar_to_string(params.get(param));
                    if val.chars().any(|c| matches!(c, '\0' | '\r' | '\n')) {
                        return Err(ContractError::InvalidScalarParam(param.clone()));
                    }
                    out.push(val);
                    *non_path_tokens += 1;
                }
            },
        }
    }
    Ok(())
}

fn format_param_template(
    template: &str,
    params: &Map<String, Value>,
) -> Result<String, ContractError> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        rendered.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            rendered.push('{');
            rest = after_open;
            continue;
        };
        let key = &after_open[..close];
        if key == "source" || key == "destination" {
            return Err(ContractError::InvalidScalarParam(key.to_owned()));
        }
        let val = scalar_to_string(params.get(key));
        if val.chars().any(|c| matches!(c, '\0' | '\r' | '\n')) {
            return Err(ContractError::InvalidScalarParam(key.to_owned()));
        }
        rendered.push_str(&val);
        rest = &after_open[close + 1..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

pub fn evaluate_condition(clause: &ConditionClause, params: &Map<String, Value>) -> bool {
    match clause {
        ConditionClause::All { all } => all.iter().all(|c| evaluate_condition(c, params)),
        ConditionClause::Any { any } => any.iter().any(|c| evaluate_condition(c, params)),
        ConditionClause::Predicate {
            param,
            eq,
            neq,
            in_list,
            ends_with,
            starts_with,
            truthy,
            non_empty,
        } => {
            let actual = params.get(param);
            if let Some(expected) = eq
                && !json_scalar_eq(actual, expected)
            {
                return false;
            }
            if let Some(unexpected) = neq
                && json_scalar_eq(actual, unexpected)
            {
                return false;
            }
            if let Some(candidates) = in_list
                && !candidates.iter().any(|c| json_scalar_eq(actual, c))
            {
                return false;
            }
            if let Some(suffix) = ends_with
                && !scalar_to_string(actual).ends_with(suffix.as_str())
            {
                return false;
            }
            if let Some(prefix) = starts_with
                && !scalar_to_string(actual).starts_with(prefix.as_str())
            {
                return false;
            }
            if let Some(expected_truthy) = truthy
                && is_truthy(actual) != *expected_truthy
            {
                return false;
            }
            if let Some(expected_non_empty) = non_empty
                && is_non_empty(actual) != *expected_non_empty
            {
                return false;
            }
            true
        }
    }
}

fn json_scalar_eq(actual: Option<&Value>, expected: &Value) -> bool {
    let Some(actual) = actual else {
        return expected.is_null();
    };
    if actual == expected {
        return true;
    }
    match (actual, expected) {
        (Value::Number(_), Value::String(_)) | (Value::String(_), Value::Number(_)) => {
            scalar_to_string(Some(actual)) == scalar_to_string(Some(expected))
        }
        (Value::Bool(b), Value::String(s)) | (Value::String(s), Value::Bool(b)) => {
            (*b && s == "true") || (!*b && s == "false")
        }
        _ => false,
    }
}

fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(s)) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        Some(Value::Array(arr)) => !arr.is_empty(),
        Some(Value::Object(obj)) => !obj.is_empty(),
    }
}

fn is_non_empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Array(arr)) => !arr.is_empty(),
        Some(Value::Object(obj)) => !obj.is_empty(),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn loads_real_plugins_repo_contracts_if_present_and_expands_video_encode()
    -> Result<(), Box<dyn std::error::Error>> {
        let raw_op = json!({
            "schemaVersion": "1.1.0",
            "id": "operation.ffmpeg.video.encode",
            "version": "1.0.0",
            "name": "Encodage",
            "providerId": "video",
            "providerKind": "plugin",
            "capability": "encode",
            "status": "validated",
            "runtime": {
                "kind": "binary",
                "tool": { "id": "ffmpeg", "version": "1.0.0" },
                "executable": "ffmpeg",
                "executionMode": "staged-output",
                "outputProfile": "transcoded_mkv",
                "defaults": { "codec": "h265_10bit", "encoder": "nvidia", "quality": 5, "speed": 2 },
                "derived": [
                    { "target": "crf", "kind": "map", "inputs": ["quality"], "table": { "5": "24" }, "default": "22" },
                    { "target": "video_encoder", "kind": "map", "inputs": ["codec", "encoder"], "table": { "h265_10bit|nvidia": "hevc_nvenc", "vp9|*": "libvpx-vp9" }, "default": "libx265" }
                ],
                "argv": [
                    "-i", { "param": "source", "as": "filePath" },
                    "-c:v", { "param": "video_encoder" },
                    {
                        "when": { "all": [{ "param": "codec", "endsWith": "_10bit" }, { "param": "encoder", "neq": "vaapi" }] },
                        "then": ["-pix_fmt", "p010le"]
                    },
                    { "param": "destination", "as": "filePath" }
                ]
            }
        });
        let contract: OperationContract = serde_json::from_value(raw_op)?;
        let OperationRuntime::Binary(bin) = &contract.runtime else {
            panic!("expected binary runtime");
        };
        let src = PathBuf::from("/tmp/in.mkv");
        let dst = PathBuf::from("/tmp/out.mkv");
        let paths = HashMap::from([("source", src.as_path()), ("destination", dst.as_path())]);
        let (argv, changed) = build_binary_argv(bin, &json!({}), &paths, None)?;
        assert!(changed);
        assert_eq!(
            argv,
            [
                "-i",
                "/tmp/in.mkv",
                "-c:v",
                "hevc_nvenc",
                "-pix_fmt",
                "p010le",
                "/tmp/out.mkv"
            ]
        );

        let overrides = Map::from_iter([
            ("encoder".to_owned(), json!("cpu")),
            ("codec".to_owned(), json!("vp9")),
        ]);
        let (fallback_argv, _) = build_binary_argv(bin, &json!({}), &paths, Some(&overrides))?;
        assert_eq!(
            fallback_argv,
            ["-i", "/tmp/in.mkv", "-c:v", "libvpx-vp9", "/tmp/out.mkv"]
        );
        Ok(())
    }

    #[test]
    fn filepath_cannot_be_spoofed_via_node_config() -> anyhow::Result<()> {
        let bin: BinaryRuntime = serde_json::from_value(json!({
            "kind": "binary",
            "executable": "dummy",
            "executionMode": "in-place",
            "argv": [{ "param": "source", "as": "filePath" }]
        }))?;
        let err = build_binary_argv(
            &bin,
            &json!({"source": "/etc/passwd"}),
            &HashMap::new(),
            None,
        );
        assert!(matches!(err, Err(ContractError::MissingPathParam(ref p)) if p == "source"));
        Ok(())
    }

    #[test]
    fn sibling_plugin_operations_all_deserialize_without_error() -> anyhow::Result<()> {
        let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        if !sibling.join("index.json").is_file() {
            return Ok(());
        }
        let mut count = 0usize;
        for entry in std::fs::read_dir(sibling.join("plugins"))? {
            let ops_dir = entry?.path().join("operations");
            if !ops_dir.is_dir() {
                continue;
            }
            for op_file in std::fs::read_dir(ops_dir)? {
                let path = op_file?.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                    let _: OperationContract = serde_json::from_value(raw)
                        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
                    count += 1;
                }
            }
        }
        assert!(count >= 15, "attendu >= 15 opérations, lu {count}");
        let reg = OperationRegistry::load_from_roots(&[sibling]);
        assert!(reg.primary_probe().is_some());
        assert!(reg.default_fallback_operation().is_some());
        Ok(())
    }

    #[test]
    fn arr_templates_resolve_and_compose_video_audio_and_subtitles_in_one_argv()
    -> anyhow::Result<()> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        let operation = root.join("plugins/video/operations/video-encode.json");
        let first_template = root.join("flows/radarr-media-normalize/manifest.json");
        if !first_template.is_file() {
            return Ok(());
        }
        let op: OperationContract = serde_json::from_reader(std::fs::File::open(operation)?)?;
        let OperationRuntime::Binary(runtime) = op.runtime else {
            anyhow::bail!("binary expected")
        };
        let paths = HashMap::from([
            ("source", Path::new("/tmp/input.mkv")),
            ("destination", Path::new("/tmp/output.mkv")),
        ]);
        let (argv, _) = build_binary_argv(
            &runtime,
            &json!({
                "video_mode": "copy", "audio_codec": "opus", "audio_language": "fra", "subtitle_language": "eng"
            }),
            &paths,
            None,
        )?;
        assert_eq!(argv.iter().filter(|arg| *arg == "-i").count(), 1);
        for pair in [
            ["-c:v", "copy"],
            ["-c:a", "libopus"],
            ["-map", "0:a:m:language:fra"],
            ["-map", "0:s:m:language:eng"],
            ["-c:s", "copy"],
        ] {
            assert!(
                argv.windows(2)
                    .any(|args| args[0] == pair[0] && args[1] == pair[1]),
                "{pair:?}"
            );
        }
        assert_eq!(argv.last().map(String::as_str), Some("/tmp/output.mkv"));
        let registry = OperationRegistry::load_from_roots(std::slice::from_ref(&root));
        for product in ["radarr", "sonarr"] {
            let template: Value = serde_json::from_reader(std::fs::File::open(
                root.join(format!("flows/{product}-media-normalize/manifest.json")),
            )?)?;
            let graph: FlowGraph = serde_json::from_value(template["definition"].clone())?;
            assert!(
                graph
                    .nodes
                    .iter()
                    .all(|node| registry.resolve_node_operation(node).is_some())
            );
            assert!(
                graph
                    .nodes
                    .iter()
                    .any(|node| node.config["after_publication"] == true)
            );
        }
        Ok(())
    }

    #[test]
    fn probe_selection_matches_file_extension_and_priority() -> anyhow::Result<()> {
        let mut reg = OperationRegistry::default();
        for op in [
            json!({
                "schemaVersion": "1.1.0",
                "id": "operation.video.probe",
                "version": "1.0.0",
                "name": "VideoProbe",
                "providerId": "video",
                "providerKind": "plugin",
                "capability": "probe",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "executable": "vprobe",
                    "executionMode": "probe-primary",
                    "probeMatch": { "extensions": ["mkv", "mp4"] },
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
            json!({
                "schemaVersion": "1.1.0",
                "id": "operation.ebook.probe",
                "version": "1.0.0",
                "name": "EbookProbe",
                "providerId": "plugin.calibre",
                "providerKind": "plugin",
                "capability": "probe",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "executable": "ebook-meta",
                    "executionMode": "probe-primary",
                    "probeMatch": { "extensions": ["epub", "mobi"], "priority": 10 },
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
            json!({
                "schemaVersion": "1.1.0",
                "id": "operation.generic.probe",
                "version": "1.0.0",
                "name": "GenericProbe",
                "providerId": "essentiel",
                "providerKind": "plugin",
                "capability": "probe",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "executable": "fileprobe",
                    "executionMode": "probe-primary",
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
            json!({
                "schemaVersion": "1.1.0",
                "id": "operation.ebook.enrich",
                "version": "1.0.0",
                "name": "EbookEnrich",
                "providerId": "plugin.calibre",
                "providerKind": "plugin",
                "capability": "probe",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "executable": "ebook-enrich",
                    "executionMode": "probe-enrich",
                    "probeMatch": { "extensions": ["epub"] },
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
        ] {
            reg.register_json(op)?;
        }

        assert_eq!(
            reg.primary_probe_for_file(Path::new("film.MKV"))
                .map(|o| o.id.as_str()),
            Some("operation.video.probe")
        );
        assert_eq!(
            reg.primary_probe_for_file(Path::new("roman.epub"))
                .map(|o| o.id.as_str()),
            Some("operation.ebook.probe")
        );
        assert_eq!(
            reg.primary_probe_for_file(Path::new("archive.zip"))
                .map(|o| o.id.as_str()),
            Some("operation.generic.probe")
        );
        assert_eq!(
            reg.enrich_probes_for_file(Path::new("roman.epub"))
                .into_iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            vec!["operation.ebook.enrich"]
        );
        assert!(reg.enrich_probes_for_file(Path::new("film.mkv")).is_empty());
        Ok(())
    }

    #[test]
    fn load_from_roots_inlines_safe_script_entrypoint_and_rejects_traversal() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join("scripts/hello/operations"))?;
        std::fs::write(dir.path().join("outside.sh"), "echo secret")?;
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"script.hello","type":"script","version":"1.0.0","name":"Hello","manifestPath":"scripts/hello/manifest.json"}
            ]}"#,
        )?;
        std::fs::write(
            dir.path().join("scripts/hello/manifest.json"),
            r#"{"schemaVersion":"1.1.0","id":"script.hello","type":"script","version":"1.0.0","name":"Hello",
                "files":[
                    {"path":"operations/run.json","role":"operation","mediaType":"application/json"},
                    {"path":"operations/evil.json","role":"operation","mediaType":"application/json"}
                ]}"#,
        )?;
        std::fs::write(
            dir.path().join("scripts/hello/operations/run.json"),
            r#"{"schemaVersion":"1.1.0","id":"operation.script.hello","version":"1.0.0","name":"Hello",
                "providerId":"script.hello","providerKind":"script","capability":"custom","status":"validated",
                "runtime":{"kind":"script","interpreter":"sh","entrypoint":"run.sh"}}"#,
        )?;
        std::fs::write(
            dir.path().join("scripts/hello/run.sh"),
            "printf '{\"status\":\"ok\"}'",
        )?;
        std::fs::write(
            dir.path().join("scripts/hello/operations/evil.json"),
            r#"{"schemaVersion":"1.1.0","id":"operation.script.evil","version":"1.0.0","name":"Evil",
                "providerId":"script.hello","providerKind":"script","capability":"custom","status":"validated",
                "runtime":{"kind":"script","interpreter":"sh","entrypoint":"../../outside.sh"}}"#,
        )?;

        let reg = OperationRegistry::load_from_roots(&[dir.path().to_path_buf()]);
        let ok_op = reg
            .get("operation.script.hello")
            .ok_or_else(|| anyhow::anyhow!("opération script manquante"))?;
        let OperationRuntime::Script(rt) = &ok_op.runtime else {
            panic!("attendu ScriptRuntime");
        };
        assert_eq!(rt.code.as_deref(), Some("printf '{\"status\":\"ok\"}'"));

        let evil_op = reg
            .get("operation.script.evil")
            .ok_or_else(|| anyhow::anyhow!("opération evil manquante"))?;
        let OperationRuntime::Script(evil_rt) = &evil_op.runtime else {
            panic!("attendu ScriptRuntime");
        };
        assert!(evil_rt.code.is_none());
        Ok(())
    }
    fn provider_operation(op_id: &str, provider: &str, namespace: &str, version: &str) -> Value {
        json!({
            "schemaVersion": "1.1.0", "id": op_id, "version": "1.0.0", "name": "Fournisseur",
            "providerId": provider, "providerKind": "plugin", "capability": "probe", "status": "validated",
            "runtime": {"kind": "script", "interpreter": "sh"},
            "exposes": {
                "namespace": namespace,
                "contractVersion": version,
                "variables": [{"name": "video.codec", "type": "text", "path": "video.codec"}]
            }
        })
    }

    fn script_operation_requiring(requires: Value) -> Value {
        json!({
            "schemaVersion": "1.1.0", "id": "operation.script.consumer", "version": "1.0.0", "name": "Consommateur",
            "providerId": "script.consumer", "providerKind": "script", "capability": "script", "status": "validated",
            "runtime": {"kind": "script", "interpreter": "sh", "requires": requires}
        })
    }

    #[test]
    fn exposures_are_registered_and_requirements_are_checked_against_the_provider()
    -> Result<(), ContractError> {
        let mut reg = OperationRegistry::new();
        reg.register_json(provider_operation(
            "operation.mi.probe",
            "app.mediainfo",
            "mediainfo",
            "1.0.0",
        ))?;
        let exposed = reg.exposed_variables();
        assert_eq!(exposed.len(), 1);
        assert_eq!(exposed[0].namespace, "mediainfo");

        let met = ScriptRequirement {
            variable: "mediainfo.video.codec".into(),
            provider: "app.mediainfo".into(),
            contract_major: 1,
            repository: None,
        };
        let wrong_provider = ScriptRequirement {
            provider: "app.autre".into(),
            ..met.clone()
        };
        let wrong_major = ScriptRequirement {
            contract_major: 2,
            ..met.clone()
        };
        let unknown = ScriptRequirement {
            variable: "mediainfo.audio.codec".into(),
            ..met.clone()
        };
        let all = [met.clone(), wrong_provider, wrong_major, unknown];
        let unmet = reg.unmet_requirements(&all);
        assert_eq!(unmet.len(), 3);
        assert!(!unmet.contains(&&met));
        Ok(())
    }

    #[test]
    fn the_first_operation_keeps_a_contested_namespace() -> Result<(), ContractError> {
        let mut reg = OperationRegistry::new();
        reg.register_json(provider_operation(
            "operation.b.probe",
            "app.b",
            "mediainfo",
            "1.0.0",
        ))?;
        reg.register_json(provider_operation(
            "operation.a.probe",
            "app.a",
            "mediainfo",
            "2.0.0",
        ))?;
        let exposed = reg.exposed_variables();
        assert_eq!(exposed.len(), 1);
        assert_eq!(exposed[0].contract_version, "2.0.0");
        Ok(())
    }

    #[test]
    fn invalid_declarations_are_refused_at_registration() {
        let mut reg = OperationRegistry::new();
        assert!(
            reg.register_json(provider_operation("operation.x", "app.x", "nodes", "1.0.0"))
                .is_err()
        );
        assert!(
            reg.register_json(provider_operation(
                "operation.x",
                "app.x",
                "mediainfo",
                "1.0"
            ))
            .is_err()
        );
        assert!(
            reg.register_json(script_operation_requiring(
                json!([{"variable": "codec", "provider": "app.x"}])
            ))
            .is_err()
        );
        assert!(reg.get("operation.x").is_none());
        assert!(
            reg.register_json(script_operation_requiring(
                json!([{"variable": "mediainfo.video.codec", "provider": "app.mediainfo"}])
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_script_dependency_picks_its_provider_even_when_another_plugin_shares_the_namespace()
    -> Result<(), ContractError> {
        let mut reg = OperationRegistry::new();
        reg.register_json(provider_operation(
            "operation.a.probe",
            "alpha",
            "mediainfo",
            "1.0.0",
        ))?;
        reg.register_json(provider_operation(
            "operation.b.probe",
            "beta",
            "mediainfo",
            "1.5.0",
        ))?;
        reg.register_json(provider_operation(
            "operation.c.probe",
            "gamma",
            "mediainfo",
            "2.0.0",
        ))?;
        let requirement = |provider: &str, major: u32| ScriptRequirement {
            variable: "mediainfo.video.codec".into(),
            provider: provider.into(),
            contract_major: major,
            repository: None,
        };

        let beta = [requirement("beta", 1)];
        let bound = reg.exposures_bound_by(&beta);
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].contract_version, "1.5.0");
        let read = reg.exposures_for_requirements(&beta);
        assert_eq!(
            read.len(),
            1,
            "l'espace déjà lié n'est pas repris par le repli"
        );
        assert_eq!(read[0].contract_version, "1.5.0");

        let gamma = [requirement("gamma", 2)];
        assert_eq!(reg.exposures_bound_by(&gamma)[0].contract_version, "2.0.0");

        let none = reg.exposures_for_requirements(&[]);
        assert_eq!(none.len(), 1);
        assert_eq!(
            none[0].contract_version, "1.0.0",
            "sans dépendance, la première l'emporte"
        );
        Ok(())
    }

    #[test]
    fn an_unmatched_dependency_binds_nothing_and_falls_back_to_the_first_exposure()
    -> Result<(), ContractError> {
        let mut reg = OperationRegistry::new();
        reg.register_json(provider_operation(
            "operation.a.probe",
            "alpha",
            "mediainfo",
            "1.0.0",
        ))?;
        let unmatched = |provider: &str, major: u32| {
            [ScriptRequirement {
                variable: "mediainfo.video.codec".into(),
                provider: provider.into(),
                contract_major: major,
                repository: None,
            }]
        };
        for requirements in [unmatched("alpha", 2), unmatched("inconnu", 1)] {
            assert!(reg.exposures_bound_by(&requirements).is_empty());
            assert_eq!(reg.exposures_for_requirements(&requirements).len(), 1);
        }
        let unknown_variable = [ScriptRequirement {
            variable: "mediainfo.audio.absente".into(),
            provider: "alpha".into(),
            contract_major: 1,
            repository: None,
        }];
        assert!(reg.exposures_bound_by(&unknown_variable).is_empty());
        Ok(())
    }

    #[test]
    fn only_literal_read_methods_are_safe() -> Result<(), serde_json::Error> {
        let runtime = |methods: &[&str]| -> Result<HttpRuntime, serde_json::Error> {
            let steps: Vec<Value> = methods
                .iter()
                .map(|method| json!({"url": "https://example.invalid", "method": method}))
                .collect();
            serde_json::from_value(json!({ "steps": steps }))
        };
        assert!(runtime(&["GET", " head ", "options"])?.is_safe());
        assert!(!runtime(&["GET", "POST"])?.is_safe());
        assert!(!runtime(&["{config.method}"])?.is_safe());
        assert!(!runtime(&["{$vars.method}"])?.is_safe());
        Ok(())
    }
}
