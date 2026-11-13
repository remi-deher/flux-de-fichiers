//! Entrée du processus et sélection de son rôle au démarrage.
//!
//! Sans argument, le rôle est celui conservé dans `/config` : un worker enrôlé reprend son rôle, tout
//! autre nœud démarre en centrale (et propose l'assistant d'installation). `coordinator` et `worker`
//! restent acceptés comme arguments pour l'automatisation.

use crate::{
    api, config,
    config::Config,
    node_settings,
    worker::{self, WorkerConfig},
};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Coordinator,
    Worker,
}

static ROLE_SWITCH_REQUESTED: AtomicBool = AtomicBool::new(false);
static ROLE_SWITCH_NOTIFY: Notify = Notify::const_new();

/// Demande l'arrêt propre de la centrale pour que le processus reparte avec le rôle enregistré
/// (appelé une fois l'enrôlement en worker terminé).
pub fn request_role_switch() {
    ROLE_SWITCH_REQUESTED.store(true, Ordering::SeqCst);
    ROLE_SWITCH_NOTIFY.notify_waiters();
}

/// Se termine quand un changement de rôle est demandé.
pub async fn role_switch_requested() {
    loop {
        let notified = ROLE_SWITCH_NOTIFY.notified();
        if ROLE_SWITCH_REQUESTED.load(Ordering::SeqCst) {
            return;
        }
        notified.await;
    }
}

/// Consomme la demande de changement de rôle : `true` si elle était en attente.
fn take_role_switch() -> bool {
    ROLE_SWITCH_REQUESTED.swap(false, Ordering::SeqCst)
}

fn parse_role(argument: Option<&str>) -> anyhow::Result<Option<Role>> {
    match argument {
        Some("worker") => Ok(Some(Role::Worker)),
        Some("coordinator") => Ok(Some(Role::Coordinator)),
        None => Ok(None),
        _ => anyhow::bail!("Usage: flux-de-fichiers [coordinator|worker|--version]"),
    }
}

/// Rôle de départ : l'argument s'il existe, sinon le rôle conservé dans `/config`.
fn initial_role(argument: Option<&str>) -> anyhow::Result<Role> {
    if let Some(role) = parse_role(argument)? {
        return Ok(role);
    }
    Ok(
        if node_settings::load_worker(&config::resolve_config_dir())?.is_some() {
            Role::Worker
        } else {
            Role::Coordinator
        },
    )
}

pub async fn run(argument: Option<&str>) -> anyhow::Result<()> {
    if argument == Some("--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut role = initial_role(argument)?;
    loop {
        match role {
            Role::Worker => {
                return worker::run(WorkerConfig::load(&config::resolve_config_dir())?).await;
            }
            Role::Coordinator => {
                api::serve(Config::load()?).await?;
                if !take_role_switch() {
                    return Ok(());
                }
                tracing::info!("changement de rôle : redémarrage en worker");
                role = initial_role(None)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arguments_select_the_role() -> anyhow::Result<()> {
        assert_eq!(parse_role(Some("worker"))?, Some(Role::Worker));
        assert_eq!(parse_role(Some("coordinator"))?, Some(Role::Coordinator));
        assert_eq!(parse_role(None)?, None);
        assert!(parse_role(Some("autre")).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_role_switch_wakes_waiters_once_requested() {
        let waiter = tokio::spawn(role_switch_requested());
        tokio::task::yield_now().await;
        request_role_switch();
        assert!(waiter.await.is_ok());
        assert!(take_role_switch());
        assert!(!take_role_switch());
    }
}
