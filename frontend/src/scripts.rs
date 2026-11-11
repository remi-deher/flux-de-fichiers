//! Script client modulaire pour le poste de pilotage Flux de Fichiers.

/// Retourne le script client consolidé généré à partir des modules Rust.
pub fn get_client_script() -> &'static str {
    crate::client::get_client_script()
}
