//! Aides partagées par les tests d'intégration : clés d'API de valeur connue.

#![allow(dead_code)]

use forgearr::api_keys::install_known_key;
use forgearr_shared::Role;
use sqlx::SqlitePool;

pub const ADMIN_KEY: &str = "fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef";
pub const WORKER_KEY: &str = "fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210";

/// Page minimale servie par les tests d'interface : la vraie page est générée, non versionnée.
pub const TEST_PAGE: &str = "<!doctype html><title>Forgearr</title>";

/// Dossier statique temporaire contenant `index.html` et la marque SVG (versionnée), à garder en
/// vie pendant le test : le central ne sert aucun fichier statique sans `index.html`.
pub fn static_dir_with_page() -> anyhow::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("index.html"), TEST_PAGE)?;
    std::fs::copy(
        "frontend/dist/forgearr.svg",
        dir.path().join("forgearr.svg"),
    )?;
    Ok(dir)
}

/// Installe une clé administrateur et une clé worker de valeur connue.
pub async fn install_keys(pool: &SqlitePool) -> anyhow::Result<()> {
    install_known_key(pool, ADMIN_KEY, "test-admin", Role::Admin).await?;
    install_known_key(pool, WORKER_KEY, "test-worker", Role::Worker).await?;
    Ok(())
}
