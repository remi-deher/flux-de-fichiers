//! Coordinateur HTTP, état PostgreSQL et traitement local intégré facultatif.

pub use crate::api::{AppState, router};

pub async fn run(config: crate::config::Config) -> anyhow::Result<()> {
    crate::api::serve(config).await
}
