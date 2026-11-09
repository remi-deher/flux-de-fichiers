//! Entrée du processus et sélection de son rôle au démarrage.

use crate::{config::Config, coordinator, workers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Coordinator,
    Worker,
}

pub async fn run(argument: Option<&str>) -> anyhow::Result<()> {
    if argument == Some("--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let role = match argument {
        Some("worker") => Role::Worker,
        Some("coordinator") | None => Role::Coordinator,
        _ => anyhow::bail!("Usage: flux-de-fichiers [coordinator|worker|--version]"),
    };
    match role {
        Role::Worker => workers::run_remote().await,
        Role::Coordinator => coordinator::run(Config::load()?).await,
    }
}
