//! Outils requis par une pièce du catalogue (dépendances transitives) et leur installation.

use super::{
    CatalogEntry, CatalogItemType, SyncPolicy, catalog_with_roots, ensure_tool_installed,
    ensure_tool_installed_verified, load_manifest, read_script_requirements,
};
use crate::{
    error::{AppError, AppResult},
    tools_cache::ToolCacheManager,
};
use forgearr_shared::exposed_variables::RepositoryPointer;
pub use forgearr_shared::plugins::DependencyInstallResult;
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

/// Outil du catalogue à installer, à la version publiée par le dépôt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredTool {
    pub id: String,
    pub version: String,
}

/// Résolution des dépendances d'une pièce : outils dans l'ordre d'installation
/// (dépendances avant dépendants) et identifiants absents du catalogue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DependencyPlan {
    pub tools: Vec<RequiredTool>,
    pub missing: Vec<String>,
    /// Dépôts désignés par les scripts pour fournir une dépendance absente ; à confirmer par
    /// l'utilisateur, jamais ajoutés automatiquement (ADR 0025).
    pub suggested_repositories: Vec<RepositoryPointer>,
}

/// Résout récursivement les dépendances de `item_id` (lui-même compris s'il est un outil).
/// Les cycles sont ignorés ; une pièce inconnue renvoie `NotFound`.
pub fn resolve_dependencies(roots: &[PathBuf], item_id: &str) -> AppResult<DependencyPlan> {
    let entries: HashMap<String, (PathBuf, CatalogEntry)> = catalog_with_roots(roots)
        .into_iter()
        .map(|(root, entry)| (entry.id.clone(), (root, entry)))
        .collect();
    if !entries.contains_key(item_id) {
        return Err(AppError::NotFound);
    }
    let mut plan = DependencyPlan::default();
    let mut visited = HashSet::new();
    visit(&entries, item_id, &mut visited, &mut plan);
    Ok(plan)
}

fn visit(
    entries: &HashMap<String, (PathBuf, CatalogEntry)>,
    id: &str,
    visited: &mut HashSet<String>,
    plan: &mut DependencyPlan,
) {
    if !visited.insert(id.to_owned()) {
        return;
    }
    let Some((root, entry)) = entries.get(id) else {
        plan.missing.push(id.to_owned());
        return;
    };
    let dependencies = load_manifest(root, &entry.manifest_path)
        .map(|manifest| manifest.dependencies)
        .unwrap_or_default();
    for dependency in &dependencies {
        visit(entries, &dependency.id, visited, plan);
    }
    if entry.entry_type == CatalogItemType::Script {
        visit_script_providers(entries, root, entry, visited, plan);
    }
    if entry.entry_type == CatalogItemType::App {
        plan.tools.push(RequiredTool {
            id: entry.id.clone(),
            version: entry.version.clone(),
        });
    }
}

/// Fournisseurs des variables exposées qu'un script consomme (sauf script « sans dépendance »).
fn visit_script_providers(
    entries: &HashMap<String, (PathBuf, CatalogEntry)>,
    root: &std::path::Path,
    entry: &CatalogEntry,
    visited: &mut HashSet<String>,
    plan: &mut DependencyPlan,
) {
    let Some(item_dir) = root
        .join(&entry.manifest_path)
        .parent()
        .map(std::path::Path::to_path_buf)
    else {
        return;
    };
    let requirements = read_script_requirements(&item_dir);
    if requirements.dependencies_unverified {
        return;
    }
    for requirement in requirements.requires {
        visit(entries, &requirement.provider, visited, plan);
        if !entries.contains_key(&requirement.provider)
            && let Some(repository) = requirement.repository
            && !plan.suggested_repositories.contains(&repository)
        {
            plan.suggested_repositories.push(repository);
        }
    }
}

/// Installe un outil ; avec une base, l'immuabilité TOFU de sa Release est aussi vérifiée.
pub(super) async fn install_tool_with_optional_pool(
    pool: Option<&SqlitePool>,
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    tool: &RequiredTool,
    policy: &SyncPolicy,
) -> AppResult<PathBuf> {
    match pool {
        Some(pool) => {
            ensure_tool_installed_verified(pool, roots, cache, &tool.id, &tool.version, policy)
                .await
        }
        None => ensure_tool_installed(roots, cache, &tool.id, &tool.version, policy).await,
    }
}

/// Installe une pièce et ses outils requis. Un échec de la pièce elle-même (outil demandé)
/// est une erreur ; ceux de ses dépendances sont rapportés dans le bilan.
pub async fn install_with_dependencies(
    pool: Option<&SqlitePool>,
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    policy: &SyncPolicy,
    item_id: &str,
) -> AppResult<DependencyInstallResult> {
    let plan = resolve_dependencies(roots, item_id)?;
    let mut result = DependencyInstallResult {
        item_id: item_id.to_owned(),
        installed_tools: Vec::new(),
        failed_tools: Vec::new(),
        missing_dependencies: plan.missing,
        suggested_repositories: plan.suggested_repositories,
    };
    for tool in &plan.tools {
        match install_tool_with_optional_pool(pool, roots, cache, tool, policy).await {
            Ok(_) => result.installed_tools.push(tool.id.clone()),
            Err(error) if tool.id == item_id => return Err(error),
            Err(error) => result
                .failed_tools
                .push((tool.id.clone(), error.to_string())),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write(root: &Path, rel: &str, body: &str) -> anyhow::Result<()> {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, body)?;
        Ok(())
    }

    fn manifest(id: &str, kind: &str, deps: &[&str]) -> String {
        let deps: Vec<_> = deps
            .iter()
            .map(|dep| serde_json::json!({ "id": dep, "optional": true }))
            .collect();
        serde_json::json!({
            "schemaVersion": "1.1.0", "id": id, "type": kind, "version": "1.0.0",
            "name": id, "dependencies": deps
        })
        .to_string()
    }

    /// Catalogue : plugin `video` → outils `ffmpeg` et `absent` ; `ffmpeg` → `libs` ;
    /// `libs` → `ffmpeg` (cycle).
    fn catalogue() -> anyhow::Result<(tempfile::TempDir, Vec<PathBuf>)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        write(
            root,
            "index.json",
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"video","type":"plugin","version":"1.0.0","name":"Vidéo","manifestPath":"plugins/video/manifest.json"},
                {"id":"ffmpeg","type":"app","version":"2.0.0","name":"FFmpeg","manifestPath":"apps/ffmpeg/manifest.json"},
                {"id":"libs","type":"app","version":"1.1.0","name":"Libs","manifestPath":"apps/libs/manifest.json"}
            ]}"#,
        )?;
        write(
            root,
            "plugins/video/manifest.json",
            &manifest("video", "plugin", &["ffmpeg", "absent"]),
        )?;
        write(
            root,
            "apps/ffmpeg/manifest.json",
            &manifest("ffmpeg", "app", &["libs"]),
        )?;
        write(
            root,
            "apps/libs/manifest.json",
            &manifest("libs", "app", &["ffmpeg"]),
        )?;
        let roots = vec![root.to_path_buf()];
        Ok((dir, roots))
    }

    #[test]
    fn resolves_transitive_tools_before_dependents_and_reports_missing() -> anyhow::Result<()> {
        let (_dir, roots) = catalogue()?;
        let plan = resolve_dependencies(&roots, "video")?;
        let ids: Vec<_> = plan.tools.iter().map(|tool| tool.id.as_str()).collect();
        assert_eq!(ids, ["libs", "ffmpeg"]);
        assert_eq!(plan.tools[1].version, "2.0.0");
        assert_eq!(plan.missing, ["absent"]);
        Ok(())
    }

    #[test]
    fn a_tool_requires_itself_after_its_dependencies() -> anyhow::Result<()> {
        let (_dir, roots) = catalogue()?;
        let plan = resolve_dependencies(&roots, "ffmpeg")?;
        let ids: Vec<_> = plan.tools.iter().map(|tool| tool.id.as_str()).collect();
        assert_eq!(ids, ["libs", "ffmpeg"]);
        assert!(plan.missing.is_empty());
        Ok(())
    }

    #[test]
    fn unknown_item_is_not_found() -> anyhow::Result<()> {
        let (_dir, roots) = catalogue()?;
        assert!(matches!(
            resolve_dependencies(&roots, "inconnu"),
            Err(AppError::NotFound)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn plugin_install_reports_failed_dependencies_without_erroring() -> anyhow::Result<()> {
        let (_dir, roots) = catalogue()?;
        let cache_dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(cache_dir.path());
        let result =
            install_with_dependencies(None, &roots, &cache, &SyncPolicy::default(), "video")
                .await?;
        assert_eq!(result.item_id, "video");
        assert!(result.installed_tools.is_empty());
        let failed: Vec<_> = result
            .failed_tools
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(failed, ["libs", "ffmpeg"]);
        assert_eq!(result.missing_dependencies, ["absent"]);
        Ok(())
    }

    #[tokio::test]
    async fn tool_install_fails_when_the_requested_tool_fails() -> anyhow::Result<()> {
        let (_dir, roots) = catalogue()?;
        let cache_dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(cache_dir.path());
        let outcome =
            install_with_dependencies(None, &roots, &cache, &SyncPolicy::default(), "ffmpeg").await;
        assert!(outcome.is_err());
        Ok(())
    }

    fn script_catalogue(
        requires: serde_json::Value,
    ) -> anyhow::Result<(tempfile::TempDir, Vec<PathBuf>)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        write(
            root,
            "index.json",
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"script.codec","type":"script","version":"1.0.0","name":"Codec","manifestPath":"scripts/codec/manifest.json"},
                {"id":"app.mediainfo","type":"app","version":"1.0.0","name":"MediaInfo","manifestPath":"apps/mediainfo/manifest.json"}
            ]}"#,
        )?;
        write(
            root,
            "scripts/codec/manifest.json",
            &manifest("script.codec", "script", &[]),
        )?;
        write(
            root,
            "apps/mediainfo/manifest.json",
            &manifest("app.mediainfo", "app", &[]),
        )?;
        write(
            root,
            "scripts/codec/operations/run.json",
            &serde_json::json!({"runtime": {"requires": requires}}).to_string(),
        )?;
        let roots = vec![root.to_path_buf()];
        Ok((dir, roots))
    }

    #[test]
    fn a_script_pulls_in_the_provider_of_the_variables_it_requires() -> anyhow::Result<()> {
        let (_dir, roots) = script_catalogue(serde_json::json!([
            {"variable": "mediainfo.video.codec", "provider": "app.mediainfo"}
        ]))?;
        let plan = resolve_dependencies(&roots, "script.codec")?;
        let ids: Vec<_> = plan.tools.iter().map(|tool| tool.id.as_str()).collect();
        assert_eq!(ids, ["app.mediainfo"]);
        assert!(plan.missing.is_empty());
        Ok(())
    }

    #[test]
    fn a_missing_provider_is_reported_with_the_repository_to_add_but_never_added()
    -> anyhow::Result<()> {
        let (_dir, roots) = script_catalogue(serde_json::json!([
            {"variable": "exiftool.camera.model", "provider": "app.exiftool",
             "repository": {"id": "repo.tiers", "indexUrl": "https://exemple.org/p/index.json"}}
        ]))?;
        let plan = resolve_dependencies(&roots, "script.codec")?;
        assert_eq!(plan.missing, ["app.exiftool"]);
        assert_eq!(plan.suggested_repositories.len(), 1);
        assert_eq!(plan.suggested_repositories[0].id, "repo.tiers");
        Ok(())
    }

    #[test]
    fn an_unverified_script_plans_no_dependency() -> anyhow::Result<()> {
        let (dir, roots) = script_catalogue(serde_json::json!([
            {"variable": "exiftool.camera.model", "provider": "app.exiftool"}
        ]))?;
        write(
            dir.path(),
            "scripts/codec/operations/run.json",
            &serde_json::json!({"runtime": {"dependenciesUnverified": true, "requires": [
                {"variable": "exiftool.camera.model", "provider": "app.exiftool"}]}})
            .to_string(),
        )?;
        let plan = resolve_dependencies(&roots, "script.codec")?;
        assert!(plan.missing.is_empty() && plan.tools.is_empty());
        Ok(())
    }
}
