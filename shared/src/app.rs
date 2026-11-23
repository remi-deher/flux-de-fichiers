//! Chemins des écrans du client web (ADR 0014), servis à la racine.

pub const OVERVIEW: &str = "/";
pub const JOBS: &str = "/jobs";
pub const FLOWS: &str = "/flows";
pub const NEW_FLOW: &str = "/flows/new";
pub const LIBRARIES: &str = "/libraries";
/// Éditeur d'un script local (ADR 0030) : `/libraries/scripts/new`, puis `/libraries/scripts/<id>`.
pub const SCRIPTS: &str = "/libraries/scripts";
pub const NEW_SCRIPT: &str = "/libraries/scripts/new";
pub const WORKERS: &str = "/workers";
pub const PLUGINS: &str = "/plugins";
pub const SETTINGS: &str = "/settings";
/// Préfixe du client pendant la transition : ses anciens liens restent valides.
pub const LEGACY_PREFIX: &str = "/app";
