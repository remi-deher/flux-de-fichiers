//! Prévisualisation en lecture seule du code d'un script exposé par un dépôt synchronisé.

use super::{
    CatalogEntry, CatalogItemType, catalog_with_roots,
    highlight::highlight_code,
    local::{read_local_script, script_name_taken},
    parse_index_tolerant, read_script_requirements, repository_dir, requirement_statuses,
};
use crate::flows::OperationRegistry;
use forgearr_shared::{
    highlight::ScriptPreviewView,
    local::{LocalScriptSummary, UpsertLocalScriptRequest},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Entrée script d'un dépôt synchronisé, avec le dossier de sa copie locale.
fn find_script_entry(
    repos_dir: &Path,
    repository_id: &str,
    entry_id: &str,
) -> Option<(PathBuf, CatalogEntry)> {
    let dir = repository_dir(repos_dir, repository_id)?;
    let content = fs::read_to_string(dir.join("index.json")).ok()?;
    let entry = parse_index_tolerant(&content)
        .ok()?
        .index
        .entries
        .into_iter()
        .find(|entry| entry.entry_type == CatalogItemType::Script && entry.id == entry_id)?;
    Some((dir, entry))
}

/// Script du catalogue (tous dépôts confondus) : dossier racine de sa copie et fiche. Le premier
/// dépôt qui porte l'identifiant l'emporte, comme dans le catalogue affiché.
fn find_catalog_script(roots: &[PathBuf], entry_id: &str) -> Option<(PathBuf, CatalogEntry)> {
    catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| entry.entry_type == CatalogItemType::Script && entry.id == entry_id)
}

/// Aperçu d'un script d'un dépôt : métadonnées, code, lignes de jetons colorés et état de ses
/// dépendances d'après les plugins chargés par `registry`.
pub fn read_repository_script_preview(
    repos_dir: &Path,
    local_dir: &Path,
    repository_id: &str,
    entry_id: &str,
    registry: &OperationRegistry,
) -> Option<ScriptPreviewView> {
    let (dir, entry) = find_script_entry(repos_dir, repository_id, entry_id)?;
    preview_of(&dir, local_dir, &entry, registry)
}

/// Aperçu d'un script du catalogue, sans connaître son dépôt.
pub fn read_catalog_script_preview(
    roots: &[PathBuf],
    local_dir: &Path,
    entry_id: &str,
    registry: &OperationRegistry,
) -> Option<ScriptPreviewView> {
    let (dir, entry) = find_catalog_script(roots, entry_id)?;
    preview_of(&dir, local_dir, &entry, registry)
}

/// Script du catalogue prêt à être enregistré dans la librairie locale (nouvel identifiant), avec le
/// dossier du dépôt qui le porte.
pub fn catalog_script_import(
    roots: &[PathBuf],
    entry_id: &str,
) -> Option<(PathBuf, UpsertLocalScriptRequest)> {
    let (dir, entry) = find_catalog_script(roots, entry_id)?;
    let script = read_local_script(&dir, &entry)?;
    Some((dir, import_request(script)))
}

/// Requête de création d'un script local qui reproduit `script` sous un identifiant neuf.
fn import_request(script: LocalScriptSummary) -> UpsertLocalScriptRequest {
    UpsertLocalScriptRequest {
        id: None,
        name: script.name,
        description: script.description,
        interpreter: script.interpreter,
        code: script.code,
        filename: Some(script.entrypoint),
        category: script.category,
        tags: script.tags,
        variables: script.variables,
        requires: script.requires,
        dependencies_unverified: script.dependencies_unverified,
    }
}

fn preview_of(
    dir: &Path,
    local_dir: &Path,
    entry: &CatalogEntry,
    registry: &OperationRegistry,
) -> Option<ScriptPreviewView> {
    let script = read_local_script(dir, entry)?;
    let item_dir = dir.join(&entry.manifest_path).parent()?.to_path_buf();
    let declared = read_script_requirements(&item_dir);
    let lines = highlight_code(&script.code, script.interpreter.default_extension());
    // Mêmes refus que l'import : script déjà local, ou nom déjà pris dans la librairie.
    let in_collection = dir == local_dir || script_name_taken(local_dir, &script.name);
    Some(ScriptPreviewView {
        in_collection,
        script,
        lines,
        requirements: requirement_statuses(&declared.requires, registry),
        dependencies_unverified: declared.dependencies_unverified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{local::rebuild_local_index, save_local_script};
    use forgearr_shared::local::{LocalScriptSummary, ScriptInterpreter, UpsertLocalScriptRequest};

    const LOCAL_DIR: &str = "local";
    const SCRIPT_CODE: &str = "cat >/dev/null\nprintf '{\"status\":\"ok\"}'";

    /// Script d'un dépôt, d'après l'aperçu (sans fournisseur chargé).
    fn read_repository_script(
        repos_dir: &Path,
        repository_id: &str,
        entry_id: &str,
    ) -> Option<LocalScriptSummary> {
        read_repository_script_preview(
            repos_dir,
            &repos_dir.join(LOCAL_DIR),
            repository_id,
            entry_id,
            &OperationRegistry::new(),
        )
        .map(|preview| preview.script)
    }

    fn repository_with_script() -> anyhow::Result<(tempfile::TempDir, String)> {
        let repos_dir = tempfile::tempdir()?;
        let repo_dir = repos_dir.path().join("repo.test");
        let script = save_local_script(
            &repo_dir,
            UpsertLocalScriptRequest {
                requires: Vec::new(),
                dependencies_unverified: false,
                variables: Vec::new(),
                id: None,
                name: "Tag MKV".into(),
                description: None,
                interpreter: ScriptInterpreter::Sh,
                code: SCRIPT_CODE.into(),
                filename: None,
                category: None,
                tags: Vec::new(),
            },
        )?;
        rebuild_local_index(&repo_dir)?;
        Ok((repos_dir, script.id))
    }

    #[test]
    fn reads_code_of_a_script_entry() -> anyhow::Result<()> {
        let (repos_dir, script_id) = repository_with_script()?;
        let script = read_repository_script(repos_dir.path(), "repo.test", &script_id)
            .ok_or_else(|| anyhow::anyhow!("script introuvable"))?;
        assert_eq!(script.code, SCRIPT_CODE);
        assert_eq!(script.entrypoint, "run.sh");
        Ok(())
    }

    #[test]
    fn preview_carries_highlighted_lines_matching_the_code() -> anyhow::Result<()> {
        let (repos_dir, script_id) = repository_with_script()?;
        let preview = read_repository_script_preview(
            repos_dir.path(),
            &repos_dir.path().join(LOCAL_DIR),
            "repo.test",
            &script_id,
            &OperationRegistry::new(),
        )
        .ok_or_else(|| anyhow::anyhow!("aperçu introuvable"))?;
        assert_eq!(preview.lines.len(), SCRIPT_CODE.lines().count());
        assert_eq!(preview.script.code, SCRIPT_CODE);
        Ok(())
    }

    #[test]
    fn preview_tells_whether_the_script_is_already_in_the_collection() -> anyhow::Result<()> {
        let (repos_dir, script_id) = repository_with_script()?;
        let local_dir = repos_dir.path().join(LOCAL_DIR);
        let roots = [repos_dir.path().join("repo.test")];
        let in_collection = || {
            read_catalog_script_preview(&roots, &local_dir, &script_id, &OperationRegistry::new())
                .map(|preview| preview.in_collection)
        };
        assert_eq!(in_collection(), Some(false));
        let (_, request) = catalog_script_import(&roots, &script_id)
            .ok_or_else(|| anyhow::anyhow!("script introuvable"))?;
        save_local_script(&local_dir, request)?;
        assert_eq!(in_collection(), Some(true));
        let local = read_catalog_script_preview(
            std::slice::from_ref(&local_dir),
            &local_dir,
            &script_id,
            &OperationRegistry::new(),
        );
        assert!(local.is_some_and(|preview| preview.in_collection));
        Ok(())
    }

    #[test]
    fn rejects_unknown_entry_unknown_repository_and_unsafe_id() -> anyhow::Result<()> {
        let (repos_dir, script_id) = repository_with_script()?;
        assert!(read_repository_script(repos_dir.path(), "repo.test", "script.absent").is_none());
        assert!(read_repository_script(repos_dir.path(), "repo.absent", &script_id).is_none());
        assert!(read_repository_script(repos_dir.path(), "../repo.test", &script_id).is_none());
        Ok(())
    }

    #[test]
    fn preview_reports_requirements_and_whether_their_provider_is_available() -> anyhow::Result<()>
    {
        let (repos_dir, script_id) = repository_with_script()?;
        let operation = repos_dir
            .path()
            .join("repo.test/scripts/tag-mkv/operations/run.json");
        let mut json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&operation)?)?;
        json["runtime"]["requires"] = serde_json::json!([
            {"variable": "mediainfo.video.codec", "provider": "app.mediainfo"}
        ]);
        fs::write(&operation, json.to_string())?;

        let preview = read_repository_script_preview(
            repos_dir.path(),
            &repos_dir.path().join(LOCAL_DIR),
            "repo.test",
            &script_id,
            &OperationRegistry::new(),
        )
        .ok_or_else(|| anyhow::anyhow!("aperçu introuvable"))?;
        assert_eq!(preview.requirements.len(), 1);
        assert!(!preview.requirements[0].available);
        assert!(!preview.dependencies_unverified);
        Ok(())
    }
}
