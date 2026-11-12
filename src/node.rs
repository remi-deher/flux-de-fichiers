//! Entrée du processus et sélection de son rôle au démarrage.

use crate::{
    api,
    config::Config,
    worker::{self, WorkerConfig},
};

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
        Role::Worker => worker::run(WorkerConfig::load()?).await,
        Role::Coordinator => api::serve(Config::load()?).await,
    }
}
