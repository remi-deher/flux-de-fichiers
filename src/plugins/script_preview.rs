//! Prévisualisation en lecture seule du code d'un script exposé par un dépôt synchronisé.

use super::{
    CatalogItemType, highlight::highlight_code, local::read_local_script, parse_index_tolerant,
    repository_dir,
};
use forgearr_shared::{highlight::ScriptPreviewView, local::LocalScriptSummary};
use std::{fs, path::Path};

/// Script d'un dépôt lu depuis sa copie locale, ou `None` si le dépôt, l'entrée ou le code manque.
pub fn read_repository_script(
    repos_dir: &Path,
    repository_id: &str,
    entry_id: &str,
) -> Option<LocalScriptSummary> {
    let dir = repository_dir(repos_dir, repository_id)?;
    let content = fs::read_to_string(dir.join("index.json")).ok()?;
    let parsed = parse_index_tolerant(&content).ok()?;
    let entry = parsed
        .index
        .entries
        .into_iter()
        .find(|entry| entry.entry_type == CatalogItemType::Script && entry.id == entry_id)?;
    read_local_script(&dir, &entry)
}

/// Aperçu d'un script d'un dépôt : métadonnées, code et lignes de jetons colorés.
pub fn read_repository_script_preview(
    repos_dir: &Path,
    repository_id: &str,
    entry_id: &str,
) -> Option<ScriptPreviewView> {
    let script = read_repository_script(repos_dir, repository_id, entry_id)?;
    let lines = highlight_code(&script.code, script.interpreter.default_extension());
    Some(ScriptPreviewView { script, lines })
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
        let preview = read_repository_script_preview(repos_dir.path(), "repo.test", &script_id)
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
}
