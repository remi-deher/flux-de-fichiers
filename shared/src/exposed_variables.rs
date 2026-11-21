//! Variables exposées par les plugins et dépendances des scripts sur elles (ADR 0025, phase C).
//!
//! Un plugin déclare `exposes` dans le contrat de son opération : un espace de noms, une version de
//! contrat et des variables dont la valeur se lit à un chemin de `mediaInfo`. Un script déclare
//! `requires` : les variables exposées qu'il consomme, avec leur fournisseur.

use crate::script_variables::{MAX_VARIABLE_NAME_LEN, VariableKind};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Espaces de noms interdits : ils désignent des racines de référence du moteur.
pub const RESERVED_NAMESPACES: &[&str] = &["nodes"];
/// Nombre maximal de variables exposées par un plugin.
pub const MAX_EXPOSED_VARIABLES: usize = 128;
/// Nombre maximal de dépendances déclarées par un script.
pub const MAX_REQUIREMENTS: usize = 32;
/// Nombre maximal de segments d'un nom ou d'un chemin.
pub const MAX_PATH_SEGMENTS: usize = 8;
/// Version majeure du contrat exposé que vise un script par défaut.
pub const DEFAULT_CONTRACT_MAJOR: u32 = 1;
const INDEX_URL_SUFFIX: &str = "/index.json";

/// Variable exposée par un plugin : son nom relatif et la requête qui lit sa valeur dans `mediaInfo`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedVariable {
    /// Nom relatif à l'espace de noms (`video.codec`).
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: VariableKind,
    /// Requête de la valeur dans `mediaInfo` après l'opération du plugin (première valeur non nulle).
    pub path: String,
}

/// Ce qu'un plugin expose aux scripts, versionné.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedVariables {
    pub namespace: String,
    /// Version du contrat exposé (`MAJEUR.MINEUR.CORRECTIF`) ; un changement cassant monte le majeur.
    pub contract_version: String,
    #[serde(default)]
    pub variables: Vec<ExposedVariable>,
}

impl ExposedVariables {
    /// Version majeure du contrat, si `contract_version` est bien formée.
    pub fn contract_major(&self) -> Option<u32> {
        parse_contract_major(&self.contract_version)
    }

    /// Référence complète d'une variable : `espace.nom`.
    pub fn reference(&self, variable: &ExposedVariable) -> String {
        format!("{}.{}", self.namespace, variable.name)
    }

    /// Ce contrat fournit-il la variable demandée, dans la version majeure visée ?
    pub fn provides(&self, requirement: &ScriptRequirement) -> bool {
        self.contract_major() == Some(requirement.contract_major)
            && self
                .variables
                .iter()
                .any(|variable| self.reference(variable) == requirement.variable)
    }
}

/// Exposition d'un plugin telle que le central la voit : son contrat et l'état de son espace de noms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedNamespaceView {
    /// Identifiant du plugin qui déclare cette exposition.
    pub provider: String,
    pub exposes: ExposedVariables,
    /// Cette exposition est lue par les scripts sans dépendance explicite : à espace de noms égal, la
    /// première l'emporte. Un script qui la désigne dans `requires` la lit quel que soit cet état.
    pub active: bool,
    /// Autres plugins qui déclarent le même espace de noms.
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

/// Pointeur vers un dépôt qui fournit une dépendance (jamais ajouté automatiquement).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPointer {
    pub id: String,
    pub index_url: String,
}

fn default_contract_major() -> u32 {
    DEFAULT_CONTRACT_MAJOR
}

/// Variable exposée qu'un script consomme, et le plugin qui la fournit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptRequirement {
    /// Référence complète (`mediainfo.video.codec`).
    pub variable: String,
    /// Identifiant du plugin fournisseur dans le catalogue.
    pub provider: String,
    #[serde(default = "default_contract_major")]
    pub contract_major: u32,
    /// Dépôt qui publie le fournisseur, s'il n'est pas celui du script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryPointer>,
}

/// Une dépendance de script et sa disponibilité sur ce central.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementStatus {
    pub requirement: ScriptRequirement,
    pub available: bool,
}

/// Raison pour laquelle une déclaration `exposes` ou `requires` est refusée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExposeError {
    InvalidNamespace(String),
    ReservedNamespace(String),
    InvalidContractVersion(String),
    TooManyVariables,
    InvalidName(String),
    DuplicateName(String),
    NestedName(String),
    InvalidPath(String),
    SecretNotExposable(String),
    TooManyRequirements,
    InvalidReference(String),
    DuplicateRequirement(String),
    /// Un même espace de noms lié à deux fournisseurs ou à deux versions majeures du contrat.
    AmbiguousNamespace(String),
    InvalidProvider(String),
    InvalidRepository(String),
}

impl fmt::Display for ExposeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNamespace(name) => write!(f, "espace de noms invalide « {name} »"),
            Self::ReservedNamespace(name) => write!(f, "espace de noms réservé « {name} »"),
            Self::InvalidContractVersion(version) => {
                write!(
                    f,
                    "version de contrat invalide « {version} » (MAJEUR.MINEUR.CORRECTIF)"
                )
            }
            Self::TooManyVariables => {
                write!(f, "au plus {MAX_EXPOSED_VARIABLES} variables exposées")
            }
            Self::InvalidName(name) => write!(f, "nom de variable exposée invalide « {name} »"),
            Self::DuplicateName(name) => {
                write!(f, "variable exposée « {name} » déclarée deux fois")
            }
            Self::NestedName(name) => {
                write!(
                    f,
                    "« {name} » est à la fois une valeur et un groupe d'autres variables"
                )
            }
            Self::InvalidPath(name) => {
                write!(f, "chemin invalide pour la variable exposée « {name} »")
            }
            Self::SecretNotExposable(name) => {
                write!(f, "la variable « {name} » ne peut pas être un secret")
            }
            Self::TooManyRequirements => write!(f, "au plus {MAX_REQUIREMENTS} dépendances"),
            Self::InvalidReference(reference) => {
                write!(
                    f,
                    "référence de dépendance invalide « {reference} » (espace.nom attendu)"
                )
            }
            Self::DuplicateRequirement(reference) => {
                write!(f, "dépendance « {reference} » déclarée deux fois")
            }
            Self::AmbiguousNamespace(namespace) => write!(
                f,
                "l'espace de noms « {namespace} » est lié à deux fournisseurs ou à deux versions de contrat différents"
            ),
            Self::InvalidProvider(provider) => write!(f, "fournisseur invalide « {provider} »"),
            Self::InvalidRepository(id) => write!(f, "dépôt de dépendance invalide « {id} »"),
        }
    }
}

impl std::error::Error for ExposeError {}

fn is_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    segment.len() <= MAX_VARIABLE_NAME_LEN
        && chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `a.b.c` : 1 à `MAX_PATH_SEGMENTS` identifiants.
fn is_dotted_identifier(name: &str) -> bool {
    let segments = name.split('.').count();
    (1..=MAX_PATH_SEGMENTS).contains(&segments) && name.split('.').all(is_identifier)
}

fn is_path_key(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Filtre `clé=valeur` d'un sélecteur (clé précédée d'un `@` facultatif, comme `@type` de MediaInfo).
fn is_filter_selector(selector: &str) -> bool {
    let Some((key, value)) = selector.split_once('=') else {
        return false;
    };
    is_path_key(key.strip_prefix('@').unwrap_or(key))
        && !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' '))
}

/// Segment d'une requête : une clé, éventuellement suivie de `[*]`, `[n]` ou `[clé=valeur]`.
fn is_path_segment(segment: &str) -> bool {
    let Some((key, rest)) = segment.split_once('[') else {
        return is_path_key(segment);
    };
    let Some(selector) = rest.strip_suffix(']') else {
        return false;
    };
    let is_index = !selector.is_empty() && selector.chars().all(|c| c.is_ascii_digit());
    is_path_key(key) && (selector == "*" || is_index || is_filter_selector(selector))
}

/// Requête dans `mediaInfo`, celle des conditions du flux : `a.b`, `liste[0]`, `liste[*]`,
/// `liste[clé=valeur]` (ex. `mediainfo.media.track[@type=Video].Format`). Un point sépare les
/// segments, la valeur d'un filtre n'en contient donc pas.
fn is_media_path(path: &str) -> bool {
    let segments = path.split('.').count();
    (1..=MAX_PATH_SEGMENTS).contains(&segments) && path.split('.').all(is_path_segment)
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_VARIABLE_NAME_LEN
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Version `X.Y.Z` en nombres : renvoie le majeur.
pub fn parse_contract_major(version: &str) -> Option<u32> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    parts.next()?.parse::<u32>().ok()?;
    parts.next()?.parse::<u32>().ok()?;
    parts.next().is_none().then_some(major)
}

/// `a` est un groupe (préfixe pointé) de `b`, ou l'inverse.
fn names_overlap(a: &str, b: &str) -> bool {
    a.strip_prefix(b).is_some_and(|rest| rest.starts_with('.'))
        || b.strip_prefix(a).is_some_and(|rest| rest.starts_with('.'))
}

/// Vérifie l'exposition d'un plugin : espace de noms sûr, version de contrat, noms et chemins valides,
/// aucune variable qui soit à la fois une valeur et un groupe.
pub fn validate_exposed(exposed: &ExposedVariables) -> Result<(), ExposeError> {
    if !is_identifier(&exposed.namespace) {
        return Err(ExposeError::InvalidNamespace(exposed.namespace.clone()));
    }
    if RESERVED_NAMESPACES.contains(&exposed.namespace.as_str()) {
        return Err(ExposeError::ReservedNamespace(exposed.namespace.clone()));
    }
    if exposed.contract_major().is_none() {
        return Err(ExposeError::InvalidContractVersion(
            exposed.contract_version.clone(),
        ));
    }
    if exposed.variables.len() > MAX_EXPOSED_VARIABLES {
        return Err(ExposeError::TooManyVariables);
    }
    for (index, variable) in exposed.variables.iter().enumerate() {
        let name = variable.name.clone();
        if !is_dotted_identifier(&variable.name) {
            return Err(ExposeError::InvalidName(name));
        }
        if !is_media_path(&variable.path) {
            return Err(ExposeError::InvalidPath(name));
        }
        if variable.kind == VariableKind::Secret {
            return Err(ExposeError::SecretNotExposable(name));
        }
        for earlier in &exposed.variables[..index] {
            if earlier.name == variable.name {
                return Err(ExposeError::DuplicateName(name));
            }
            if names_overlap(&earlier.name, &variable.name) {
                return Err(ExposeError::NestedName(name));
            }
        }
    }
    Ok(())
}

fn index_url_is_valid(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.ends_with(INDEX_URL_SUFFIX)
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Vérifie les dépendances d'un script : références `espace.nom` uniques, fournisseur et dépôt sûrs.
pub fn validate_requirements(requirements: &[ScriptRequirement]) -> Result<(), ExposeError> {
    if requirements.len() > MAX_REQUIREMENTS {
        return Err(ExposeError::TooManyRequirements);
    }
    for (index, requirement) in requirements.iter().enumerate() {
        let reference = requirement.variable.clone();
        if !is_dotted_identifier(&requirement.variable) || !requirement.variable.contains('.') {
            return Err(ExposeError::InvalidReference(reference));
        }
        if requirements[..index]
            .iter()
            .any(|other| other.variable == requirement.variable)
        {
            return Err(ExposeError::DuplicateRequirement(reference));
        }
        if !is_safe_id(&requirement.provider) {
            return Err(ExposeError::InvalidProvider(requirement.provider.clone()));
        }
        let namespace = requirement.variable.split('.').next().unwrap_or_default();
        let binds_another_contract = requirements[..index].iter().any(|other| {
            other.variable.split('.').next().unwrap_or_default() == namespace
                && (other.provider != requirement.provider
                    || other.contract_major != requirement.contract_major)
        });
        if binds_another_contract {
            return Err(ExposeError::AmbiguousNamespace(namespace.to_owned()));
        }
        if let Some(repository) = &requirement.repository
            && (!is_safe_id(&repository.id) || !index_url_is_valid(&repository.index_url))
        {
            return Err(ExposeError::InvalidRepository(repository.id.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exposed_variable(name: &str, path: &str) -> ExposedVariable {
        ExposedVariable {
            name: name.into(),
            label: String::new(),
            description: None,
            kind: VariableKind::Text,
            path: path.into(),
        }
    }

    fn mediainfo() -> ExposedVariables {
        ExposedVariables {
            namespace: "mediainfo".into(),
            contract_version: "1.2.0".into(),
            variables: vec![
                exposed_variable("video.codec", "mediainfo.video.codec"),
                exposed_variable("audio.channels", "mediainfo.audio.channels"),
            ],
        }
    }

    fn requirement(variable: &str) -> ScriptRequirement {
        ScriptRequirement {
            variable: variable.into(),
            provider: "app.mediainfo".into(),
            contract_major: 1,
            repository: None,
        }
    }

    #[test]
    fn a_valid_exposure_round_trips_through_json() -> Result<(), serde_json::Error> {
        let raw = r#"{"namespace":"mediainfo","contractVersion":"1.0.0","variables":[{"name":"video.codec","type":"text","path":"video.codec"}]}"#;
        let parsed: ExposedVariables = serde_json::from_str(raw)?;
        assert_eq!(validate_exposed(&parsed), Ok(()));
        assert_eq!(
            parsed.reference(&parsed.variables[0]),
            "mediainfo.video.codec"
        );
        let again: ExposedVariables = serde_json::from_str(&serde_json::to_string(&parsed)?)?;
        assert_eq!(again, parsed);
        assert_eq!(validate_exposed(&mediainfo()), Ok(()));
        Ok(())
    }

    #[test]
    fn namespace_and_version_are_checked() {
        let mut exposed = mediainfo();
        for bad in ["", "MediaInfo", "1media", "media-info", "media.info"] {
            exposed.namespace = bad.into();
            assert_eq!(
                validate_exposed(&exposed),
                Err(ExposeError::InvalidNamespace(bad.into()))
            );
        }
        exposed.namespace = "nodes".into();
        assert_eq!(
            validate_exposed(&exposed),
            Err(ExposeError::ReservedNamespace("nodes".into()))
        );
        exposed.namespace = "mediainfo".into();
        for bad in ["", "1", "1.0", "1.0.0.0", "a.b.c", "1.0.x"] {
            exposed.contract_version = bad.into();
            assert_eq!(
                validate_exposed(&exposed),
                Err(ExposeError::InvalidContractVersion(bad.into()))
            );
        }
    }

    #[test]
    fn names_paths_and_kinds_are_checked() {
        let mut exposed = mediainfo();
        exposed.variables[0].name = "Video.Codec".into();
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::InvalidName(_))
        ));

        let mut exposed = mediainfo();
        exposed.variables[0].path = "video..codec".into();
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::InvalidPath(_))
        ));
        exposed.variables[0].path = "video codec".into();
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::InvalidPath(_))
        ));
        exposed.variables[0].path = "streams.0.Codec-ID".into();
        assert_eq!(validate_exposed(&exposed), Ok(()));

        let mut exposed = mediainfo();
        exposed.variables[0].kind = VariableKind::Secret;
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::SecretNotExposable(_))
        ));
    }

    #[test]
    fn engine_query_selectors_are_valid_paths_and_malformed_ones_are_not() {
        for path in [
            "a.b",
            "liste[0]",
            "liste[*].b",
            "a[0].b[1]",
            "media.track[@type=Video].Format",
            "t[k=un deux]",
            "t[k-1=a_b]",
        ] {
            let mut exposed = mediainfo();
            exposed.variables[0].path = path.into();
            assert_eq!(validate_exposed(&exposed), Ok(()), "{path}");
        }
        for path in [
            "a[", "a[]", "a[0", "a[=x]", "a[k=]", "a[x y]", "a[b[0]]", "a[0]x", "[0]", "a[@@k=v]",
            "a[k=v.w]", "a[k=v]]", "a[-1]",
        ] {
            let mut exposed = mediainfo();
            exposed.variables[0].path = path.into();
            assert!(
                matches!(validate_exposed(&exposed), Err(ExposeError::InvalidPath(_))),
                "{path}"
            );
        }
    }

    /// Variables que le dépôt officiel de plugins fait exposer à MediaInfo : le central doit les accepter.
    #[test]
    fn the_official_mediainfo_exposures_are_accepted() {
        const TRACK: &str = "mediainfo.media.track";
        let variables = [
            ("general.format", "General", "Format"),
            ("general.duration", "General", "Duration"),
            ("general.size", "General", "FileSize"),
            ("general.bitrate", "General", "OverallBitRate"),
            ("video.codec", "Video", "Format"),
            ("video.width", "Video", "Width"),
            ("video.height", "Video", "Height"),
            ("video.framerate", "Video", "FrameRate"),
            ("video.bitdepth", "Video", "BitDepth"),
            ("video.hdr", "Video", "HDR_Format"),
            ("audio.codec", "Audio", "Format"),
            ("audio.channels", "Audio", "Channels"),
            ("audio.language", "Audio", "Language"),
        ]
        .map(|(name, track, field)| {
            exposed_variable(name, &format!("{TRACK}[@type={track}].{field}"))
        });
        let mut exposure = mediainfo();
        exposure.variables = variables.to_vec();
        assert_eq!(validate_exposed(&exposure), Ok(()));
    }

    #[test]
    fn duplicates_and_value_group_conflicts_are_refused() {
        let mut exposed = mediainfo();
        exposed.variables.push(exposed_variable("video.codec", "x"));
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::DuplicateName(_))
        ));

        let mut exposed = mediainfo();
        exposed.variables.push(exposed_variable("video", "video"));
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::NestedName(_))
        ));

        let mut exposed = mediainfo();
        exposed
            .variables
            .push(exposed_variable("video.codec.id", "x"));
        assert!(matches!(
            validate_exposed(&exposed),
            Err(ExposeError::NestedName(_))
        ));

        let mut exposed = mediainfo();
        exposed
            .variables
            .push(exposed_variable("video.codecs", "x"));
        assert_eq!(validate_exposed(&exposed), Ok(()));
    }

    #[test]
    fn the_variable_count_is_capped() {
        let mut exposed = mediainfo();
        exposed.variables = (0..=MAX_EXPOSED_VARIABLES)
            .map(|n| exposed_variable(&format!("v{n}"), "x"))
            .collect();
        assert_eq!(
            validate_exposed(&exposed),
            Err(ExposeError::TooManyVariables)
        );
    }

    #[test]
    fn a_contract_provides_a_requirement_only_in_the_targeted_major() {
        let exposed = mediainfo();
        assert!(exposed.provides(&requirement("mediainfo.video.codec")));
        assert!(!exposed.provides(&requirement("mediainfo.video.bitrate")));
        let mut next_major = requirement("mediainfo.video.codec");
        next_major.contract_major = 2;
        assert!(!exposed.provides(&next_major));
        assert_eq!(parse_contract_major("3.0.1"), Some(3));
        assert_eq!(parse_contract_major("3.0"), None);
    }

    #[test]
    fn requirements_default_to_the_first_major_and_are_validated() -> Result<(), serde_json::Error>
    {
        let raw = r#"[{"variable":"mediainfo.video.codec","provider":"app.mediainfo"}]"#;
        let parsed: Vec<ScriptRequirement> = serde_json::from_str(raw)?;
        assert_eq!(parsed[0].contract_major, DEFAULT_CONTRACT_MAJOR);
        assert_eq!(validate_requirements(&parsed), Ok(()));

        for bad in ["codec", "Mediainfo.codec", "media info.codec", ""] {
            assert_eq!(
                validate_requirements(&[requirement(bad)]),
                Err(ExposeError::InvalidReference(bad.into()))
            );
        }
        assert!(matches!(
            validate_requirements(&[requirement("a.b"), requirement("a.b")]),
            Err(ExposeError::DuplicateRequirement(_))
        ));
        let mut provider = requirement("a.b");
        provider.provider = "../x".into();
        assert!(matches!(
            validate_requirements(&[provider]),
            Err(ExposeError::InvalidProvider(_))
        ));
        Ok(())
    }

    #[test]
    fn a_repository_pointer_needs_a_safe_id_and_an_index_url() {
        let pointer = |id: &str, url: &str| {
            let mut req = requirement("a.b");
            req.repository = Some(RepositoryPointer {
                id: id.into(),
                index_url: url.into(),
            });
            req
        };
        assert_eq!(
            validate_requirements(&[pointer("repo.tiers", "https://exemple.org/p/index.json")]),
            Ok(())
        );
        for (id, url) in [
            ("../x", "https://exemple.org/index.json"),
            ("repo", "ftp://exemple.org/index.json"),
            ("repo", "https://exemple.org/autre.json"),
            ("repo", "https://exemple.org/a b/index.json"),
        ] {
            assert!(matches!(
                validate_requirements(&[pointer(id, url)]),
                Err(ExposeError::InvalidRepository(_))
            ));
        }
    }

    #[test]
    fn the_requirement_count_is_capped() {
        let many: Vec<ScriptRequirement> = (0..=MAX_REQUIREMENTS)
            .map(|n| requirement(&format!("ns.v{n}")))
            .collect();
        assert_eq!(
            validate_requirements(&many),
            Err(ExposeError::TooManyRequirements)
        );
    }

    #[test]
    fn one_namespace_cannot_be_bound_to_two_providers_or_two_contract_versions() {
        let from = |variable: &str, provider: &str, major: u32| ScriptRequirement {
            variable: variable.into(),
            provider: provider.into(),
            contract_major: major,
            repository: None,
        };
        let same = [
            from("mediainfo.video.codec", "app.mediainfo", 1),
            from("mediainfo.audio.codec", "app.mediainfo", 1),
        ];
        assert_eq!(validate_requirements(&same), Ok(()));
        let other_namespaces = [
            from("mediainfo.video.codec", "app.mediainfo", 1),
            from("exiftool.camera", "app.exiftool", 1),
        ];
        assert_eq!(validate_requirements(&other_namespaces), Ok(()));
        let two_providers = [
            from("mediainfo.video.codec", "app.mediainfo", 1),
            from("mediainfo.audio.codec", "app.fork", 1),
        ];
        assert_eq!(
            validate_requirements(&two_providers),
            Err(ExposeError::AmbiguousNamespace("mediainfo".into()))
        );
        let two_majors = [
            from("mediainfo.video.codec", "app.mediainfo", 1),
            from("mediainfo.audio.codec", "app.mediainfo", 2),
        ];
        assert_eq!(
            validate_requirements(&two_majors),
            Err(ExposeError::AmbiguousNamespace("mediainfo".into()))
        );
    }
}
