//! Aides partagées par les tests d'intégration : clés d'API de valeur connue.

#![allow(dead_code)]

use flux_de_fichiers::api_keys::install_known_key;
use flux_shared::Role;
use sqlx::SqlitePool;

pub const ADMIN_KEY: &str = "fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef";
pub const WORKER_KEY: &str = "fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210";

/// Installe une clé administrateur et une clé worker de valeur connue.
pub async fn install_keys(pool: &SqlitePool) -> anyhow::Result<()> {
    install_known_key(pool, ADMIN_KEY, "test-admin", Role::Admin).await?;
    install_known_key(pool, WORKER_KEY, "test-worker", Role::Worker).await?;
    Ok(())
}
