//! Prévisualisation en lecture seule du code d'un script exposé par un dépôt synchronisé.

use super::{
    CatalogEntry, CatalogItemType, highlight::highlight_code, local::read_local_script,
    parse_index_tolerant, read_script_requirements, repository_dir, requirement_statuses,
};
use crate::flows::OperationRegistry;
use forgearr_shared::{highlight::ScriptPreviewView, local::LocalScriptSummary};
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

/// Script d'un dépôt lu depuis sa copie locale, ou `None` si le dépôt, l'entrée ou le code manque.
pub fn read_repository_script(
    repos_dir: &Path,
    repository_id: &str,
    entry_id: &str,
) -> Option<LocalScriptSummary> {
    let (dir, entry) = find_script_entry(repos_dir, repository_id, entry_id)?;
    read_local_script(&dir, &entry)
}

/// Aperçu d'un script d'un dépôt : métadonnées, code, lignes de jetons colorés et état de ses
/// dépendances d'après les plugins chargés par `registry`.
pub fn read_repository_script_preview(
    repos_dir: &Path,
    repository_id: &str,
    entry_id: &str,
    registry: &OperationRegistry,
) -> Option<ScriptPreviewView> {
    let (dir, entry) = find_script_entry(repos_dir, repository_id, entry_id)?;
    let script = read_local_script(&dir, &entry)?;
    let item_dir = dir.join(&entry.manifest_path).parent()?.to_path_buf();
    let declared = read_script_requirements(&item_dir);
    let lines = highlight_code(&script.code, script.interpreter.default_extension());
    Some(ScriptPreviewView {
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
    use forgearr_shared::local::{ScriptInterpreter, UpsertLocalScriptRequest};

    const SCRIPT_CODE: &str = "cat >/dev/null\nprintf '{\"status\":\"ok\"}'";

    fn repository_with_script() -> anyhow::Result<(tempfile::TempDir, String)> {
        let repos_dir = tempfile::tempdir()?;
        let repo_dir = repos_dir.path().join("repo.test");
        let script = save_local_script(
            &repo_dir,
            UpsertLocalScriptRequest {
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
