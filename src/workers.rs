//! Rôles et exécuteurs worker intégrés ou distants.

pub use crate::worker::WorkerConfig;

pub async fn run_remote() -> anyhow::Result<()> {
    crate::worker::run(WorkerConfig::load()?).await
}

pub(crate) use crate::local_worker::run as run_local;
