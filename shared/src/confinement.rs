//! Profil de confinement d'un processus externe (ADR 0034).
//!
//! Le profil décrit ce qu'un processus lancé par le central ou par un worker a le droit de faire. Il est
//! sérialisable pour qu'un manifeste de plugin ou l'interface puisse un jour le porter ; l'application des
//! restrictions vit dans l'application (`forgearr::confine`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const BYTES_PER_MIB: u64 = 1024 * 1024;
const BYTES_PER_GIB: u64 = 1024 * BYTES_PER_MIB;

/// Mémoire de données autorisée par défaut.
pub const DEFAULT_MEMORY_BYTES: u64 = BYTES_PER_GIB;
/// Taille maximale d'un fichier écrit, par défaut.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 4 * BYTES_PER_GIB;
/// Temps processeur autorisé par défaut.
pub const DEFAULT_CPU_SECONDS: u64 = 3600;
/// Descripteurs de fichier ouverts autorisés par défaut.
pub const DEFAULT_OPEN_FILES: u64 = 1024;

/// Première version de Landlock qui filtre aussi le réseau TCP (noyau 6.7).
pub const LANDLOCK_ABI_WITH_NETWORK: u32 = 4;

/// Ce que tout interpréteur ou outil lit pour démarrer : bibliothèques, programmes système, certificats,
/// aléa. Les profils d'usage (ADR 0034) l'élargissent de leurs propres dossiers.
pub const RUNTIME_READ_PATHS: &[&str] = &[
    "/usr",
    "/lib",
    "/lib64",
    "/bin",
    "/sbin",
    "/etc",
    "/proc/self",
    "/dev/urandom",
    "/dev/zero",
];
/// Fichiers spéciaux que tout processus peut écrire.
pub const RUNTIME_WRITE_PATHS: &[&str] = &["/dev/null"];

/// Accès au réseau d'un processus confiné. Landlock ne filtre ni UDP ni les adresses : tout ou rien.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkAccess {
    #[default]
    None,
    Any,
}

/// Conduite quand le noyau ne sait pas appliquer une restriction demandée.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Enforcement {
    /// Le lancement échoue avec la raison : jamais d'exécution sans confinement en silence.
    #[default]
    Require,
    /// Les restrictions inapplicables sont ignorées et signalées sur l'erreur standard.
    BestEffort,
}

/// Plafonds appliqués par `setrlimit` ; `None` laisse la limite héritée.
///
/// La mémoire plafonne les données du processus (`RLIMIT_DATA`), pas l'espace d'adressage : V8 et les
/// serveurs de langage réservent beaucoup d'adresses sans les utiliser. Le nombre de processus n'a pas de
/// plafond ici : `RLIMIT_NPROC` compte tous les processus de l'utilisateur, central compris.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResourceLimits {
    pub memory_bytes: Option<u64>,
    pub max_file_bytes: Option<u64>,
    pub cpu_seconds: Option<u64>,
    pub open_files: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_bytes: Some(DEFAULT_MEMORY_BYTES),
            max_file_bytes: Some(DEFAULT_MAX_FILE_BYTES),
            cpu_seconds: Some(DEFAULT_CPU_SECONDS),
            open_files: Some(DEFAULT_OPEN_FILES),
        }
    }
}

/// Ce qu'un processus confiné peut faire. Le profil par défaut n'autorise rien : ni fichier, ni réseau.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfinementProfile {
    /// Dossiers et fichiers lisibles et exécutables.
    pub read_paths: Vec<PathBuf>,
    /// Dossiers et fichiers lisibles et inscriptibles.
    pub write_paths: Vec<PathBuf>,
    pub network: NetworkAccess,
    pub limits: ResourceLimits,
    pub enforcement: Enforcement,
}

impl ConfinementProfile {
    /// Le profil par défaut, avec de quoi démarrer un programme (`RUNTIME_READ_PATHS`).
    pub fn runtime() -> Self {
        let mut profile = Self::default();
        profile
            .read_paths
            .extend(RUNTIME_READ_PATHS.iter().map(PathBuf::from));
        profile
            .write_paths
            .extend(RUNTIME_WRITE_PATHS.iter().map(PathBuf::from));
        profile
    }

    pub fn reading(mut self, path: impl Into<PathBuf>) -> Self {
        self.read_paths.push(path.into());
        self
    }

    pub fn writing(mut self, path: impl Into<PathBuf>) -> Self {
        self.write_paths.push(path.into());
        self
    }

    pub fn with_network(mut self, network: NetworkAccess) -> Self {
        self.network = network;
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_enforcement(mut self, enforcement: Enforcement) -> Self {
        self.enforcement = enforcement;
        self
    }
}

/// Ce que le noyau de cette machine sait appliquer ; un worker le publie avec sa télémétrie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfinementSupport {
    /// Version de Landlock, 0 s'il est absent.
    pub landlock_abi: u32,
    pub seccomp: bool,
}

impl ConfinementSupport {
    pub const fn filters_files(self) -> bool {
        self.landlock_abi >= 1
    }

    pub const fn filters_network(self) -> bool {
        self.landlock_abi >= LANDLOCK_ABI_WITH_NETWORK
    }

    /// Toutes les restrictions du profil sont applicables.
    pub const fn is_complete(self) -> bool {
        self.filters_network() && self.seccomp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_profile_allows_nothing() {
        let profile = ConfinementProfile::default();
        assert!(profile.read_paths.is_empty());
        assert!(profile.write_paths.is_empty());
        assert_eq!(profile.network, NetworkAccess::None);
        assert_eq!(profile.enforcement, Enforcement::Require);
    }

    #[test]
    fn the_default_limits_are_the_named_constants() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.memory_bytes, Some(DEFAULT_MEMORY_BYTES));
        assert_eq!(limits.max_file_bytes, Some(DEFAULT_MAX_FILE_BYTES));
        assert_eq!(limits.cpu_seconds, Some(DEFAULT_CPU_SECONDS));
        assert_eq!(limits.open_files, Some(DEFAULT_OPEN_FILES));
    }

    #[test]
    fn the_runtime_profile_reads_the_system_and_writes_only_dev_null() {
        let profile = ConfinementProfile::runtime();
        assert!(profile.read_paths.contains(&PathBuf::from("/usr")));
        assert_eq!(profile.write_paths, vec![PathBuf::from("/dev/null")]);
        assert_eq!(profile.network, NetworkAccess::None);
    }

    #[test]
    fn the_builders_add_paths_and_settings() {
        let limits = ResourceLimits {
            memory_bytes: None,
            ..ResourceLimits::default()
        };
        let profile = ConfinementProfile::default()
            .reading("/tools/ty")
            .writing("/tmp/work")
            .with_network(NetworkAccess::Any)
            .with_limits(limits)
            .with_enforcement(Enforcement::BestEffort);
        assert_eq!(profile.read_paths, vec![PathBuf::from("/tools/ty")]);
        assert_eq!(profile.write_paths, vec![PathBuf::from("/tmp/work")]);
        assert_eq!(profile.network, NetworkAccess::Any);
        assert_eq!(profile.limits.memory_bytes, None);
        assert_eq!(profile.enforcement, Enforcement::BestEffort);
    }

    #[test]
    fn a_profile_survives_json_with_camel_case_fields() -> serde_json::Result<()> {
        let profile = ConfinementProfile::runtime().reading("/tools");
        let json = serde_json::to_string(&profile)?;
        assert!(json.contains("\"readPaths\""));
        assert!(json.contains("\"network\":\"none\""));
        assert!(json.contains("\"enforcement\":\"require\""));
        assert_eq!(serde_json::from_str::<ConfinementProfile>(&json)?, profile);
        Ok(())
    }

    #[test]
    fn missing_fields_fall_back_to_the_denying_defaults() -> serde_json::Result<()> {
        let profile: ConfinementProfile = serde_json::from_str("{}")?;
        assert_eq!(profile, ConfinementProfile::default());
        let json = r#"{"limits":{"cpuSeconds":5}}"#;
        let partial: ConfinementProfile = serde_json::from_str(json)?;
        assert_eq!(partial.limits.cpu_seconds, Some(5));
        assert_eq!(partial.limits.memory_bytes, Some(DEFAULT_MEMORY_BYTES));
        Ok(())
    }

    #[test]
    fn a_null_limit_means_unlimited() -> serde_json::Result<()> {
        let json = r#"{"limits":{"memoryBytes":null}}"#;
        let profile: ConfinementProfile = serde_json::from_str(json)?;
        assert_eq!(profile.limits.memory_bytes, None);
        Ok(())
    }

    #[test]
    fn support_levels_follow_the_landlock_version() {
        let none = ConfinementSupport::default();
        assert!(!none.filters_files() && !none.filters_network() && !none.is_complete());
        let files = ConfinementSupport {
            landlock_abi: 1,
            seccomp: true,
        };
        assert!(files.filters_files() && !files.filters_network() && !files.is_complete());
        let complete = ConfinementSupport {
            landlock_abi: LANDLOCK_ABI_WITH_NETWORK,
            seccomp: true,
        };
        assert!(complete.is_complete());
        let without_seccomp = ConfinementSupport {
            seccomp: false,
            ..complete
        };
        assert!(!without_seccomp.is_complete());
    }
}
