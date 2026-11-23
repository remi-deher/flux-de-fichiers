//! Fichiers d'un élément dans un dossier de catalogue, son empreinte et ce qui lui manque.

use super::paths::{declared_path, folder_of, read_inside, safe_path};
use crate::{
    error::{AppError, AppResult},
    opaque_token::sha256_hex,
    plugins::{
        CatalogEntry, CatalogItemType, icon::declared_icon_path, install::DISTRIBUTION_FILE_NAME,
        is_valid_repository_id, parse_index_tolerant, sources::INDEX_RELATIVE_PATH,
        sync::MAX_FILES, sync::verify_optional_sha256, vocab::CatalogSection,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::ErrorKind,
    path::{Path, PathBuf},
};

const MAX_ITEM_BYTES: usize = 64 * 1024 * 1024;
/// Champs d'index qui changent sans que le contenu change, ou que l'empreinte couvre déjà.
const VOLATILE_ENTRY_FIELDS: [&str; 3] = ["updatedAt", "sha256", "contentSha256"];

/// Rôles de `files[]` distingués ici ; les autres sont des fichiers annexes.
#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DeclaredRole {
    Operation,
    CapabilityProbe,
    Asset,
    Docs,
}

/// Le moteur lit-il ce fichier ?
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    Required,
    Optional,
}

/// Contenu d'un élément, prêt à être figé.
#[derive(Clone, Debug)]
pub struct ItemContent {
    pub entry: CatalogEntry,
    /// Fichiers que le moteur lit et que le dossier n'a pas.
    pub missing: Vec<String>,
    /// Fichiers annexes absents ou refusés.
    pub skipped: Vec<String>,
    /// Empreinte de l'entrée, de ses blocs et de tous les fichiers.
    pub content_sha256: String,
    pub(super) schema_version: String,
    /// Entrées d'index de l'élément et de ses blocs, telles que publiées.
    pub(super) raw_entries: Vec<Value>,
    files: BTreeMap<String, Vec<u8>>,
}

impl ItemContent {
    /// Fichiers, par chemin relatif à la racine du dossier de catalogue.
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    /// Mêmes fichiers, relativement au dossier du manifeste ? Sert à savoir si une copie modifiable est
    /// restée identique à sa copie figée, que l'index décrit autrement.
    pub fn has_same_files_as(&self, other: &Self) -> bool {
        self.relative_files() == other.relative_files()
    }

    fn relative_files(&self) -> BTreeMap<&str, &[u8]> {
        let manifest = safe_path(&self.entry.manifest_path).unwrap_or_default();
        let folder = folder_of(&manifest);
        self.files
            .iter()
            .map(|(path, content)| {
                let relative = path
                    .strip_prefix(folder)
                    .and_then(|rest| rest.strip_prefix('/'))
                    .unwrap_or(path);
                (relative, content.as_slice())
            })
            .collect()
    }
}

/// Lit un élément autonome (ni pack ni bloc) de ce dossier de catalogue, blocs compris.
///
/// Chaque chemin est contrôlé et chaque empreinte publiée vérifiée. Un fichier manquant n'est pas une
/// erreur : il est signalé dans `missing` ou `skipped`.
pub fn read_item(root: &Path, item_id: &str) -> AppResult<ItemContent> {
    if !is_valid_repository_id(item_id) {
        return Err(AppError::Invalid);
    }
    let root = std::fs::canonicalize(root).map_err(absent_is_not_found)?;
    let text =
        std::fs::read_to_string(root.join(INDEX_RELATIVE_PATH)).map_err(absent_is_not_found)?;
    let index = parse_index_tolerant(&text).map_err(invalid)?.index;
    let document: Value = serde_json::from_str(&text).map_err(invalid)?;
    let entries = item_entries(&document, item_id);
    let entry = entries
        .iter()
        .map(|(_, entry)| entry)
        .find(|entry| entry.id == item_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    if !entry.entry_type.is_installed_by_itself() {
        return Err(AppError::Invalid);
    }

    let mut expected: HashMap<String, String> = index
        .files_sha256
        .into_iter()
        .filter_map(|(path, sha)| Some((safe_path(&path)?, sha)))
        .collect();
    expected.extend(
        entries.iter().filter_map(|(_, entry)| {
            Some((safe_path(&entry.manifest_path)?, entry.sha256.clone()?))
        }),
    );
    let mut collector = Collector {
        root,
        expected,
        files: BTreeMap::new(),
        total_bytes: 0,
        missing: BTreeSet::new(),
        skipped: BTreeSet::new(),
    };
    collector.take_item(&entry)?;
    for (_, block) in entries.iter().filter(|(_, other)| other.id != entry.id) {
        match safe_path(&block.manifest_path) {
            Some(path) => {
                collector.take(&path, Need::Required)?;
            }
            None => collector.note_absent(&block.manifest_path, Need::Required),
        }
    }
    if collector.files.is_empty() {
        return Err(AppError::InvalidManifest(format!(
            "{item_id} : aucun fichier lisible"
        )));
    }
    let raw_entries: Vec<Value> = entries.into_iter().map(|(raw, _)| raw).collect();
    let content_sha256 = fingerprint(&raw_entries, &collector.files);
    let Collector {
        files,
        missing,
        skipped,
        ..
    } = collector;
    Ok(ItemContent {
        entry,
        skipped: skipped.difference(&missing).cloned().collect(),
        missing: missing.into_iter().collect(),
        content_sha256,
        schema_version: index.schema_version,
        raw_entries,
        files,
    })
}

fn invalid(error: impl std::fmt::Display) -> AppError {
    AppError::InvalidManifest(error.to_string())
}

fn absent_is_not_found(error: std::io::Error) -> AppError {
    match error.kind() {
        ErrorKind::NotFound => AppError::NotFound,
        _ => AppError::Storage(error),
    }
}

/// Entrée de l'élément (la première de cet identifiant) et celles de ses blocs, dans l'ordre de l'index.
fn item_entries(document: &Value, item_id: &str) -> Vec<(Value, CatalogEntry)> {
    let readable: Vec<(&Value, CatalogEntry)> = document
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|raw| Some((raw, CatalogEntry::deserialize(raw).ok()?)))
        .collect();
    let owns_blocks = readable
        .iter()
        .find(|(_, entry)| entry.id == item_id)
        .is_some_and(|(_, entry)| {
            matches!(
                entry.entry_type,
                CatalogItemType::Plugin | CatalogItemType::Script
            )
        });
    let mut seen = HashSet::new();
    readable
        .into_iter()
        .filter(|(_, entry)| {
            let is_own_block = owns_blocks
                && entry.entry_type == CatalogItemType::Block
                && entry.owner.as_deref() == Some(item_id);
            (entry.id == item_id || is_own_block) && seen.insert(entry.id.clone())
        })
        .map(|(raw, entry)| (raw.clone(), entry))
        .collect()
}

/// Les clés d'un objet JSON sont triées (`serde_json` sans `preserve_order`) : le texte est stable.
fn fingerprint(raw_entries: &[Value], files: &BTreeMap<String, Vec<u8>>) -> String {
    let entries = raw_entries.iter().map(|raw| {
        let mut stable = raw.clone();
        if let Some(fields) = stable.as_object_mut() {
            for name in VOLATILE_ENTRY_FIELDS {
                fields.remove(name);
            }
        }
        format!("entry {stable}")
    });
    let contents = files
        .iter()
        .map(|(path, content)| format!("file {} {path}", sha256_hex(content)));
    sha256_hex(entries.chain(contents).collect::<Vec<_>>().join("\n"))
}

/// Fichier de code d'une opération `script` qui ne porte pas son code.
fn script_entrypoint(operation: &Value) -> Option<&str> {
    let runtime = operation.get("runtime")?;
    let text = |key: &str| runtime.get(key).and_then(Value::as_str);
    if text("kind") != Some("script") || text("code").is_some_and(|code| !code.is_empty()) {
        return None;
    }
    text("entrypoint")
}

struct Collector {
    /// Canonique : un chemin qui passe par un lien symbolique s'en écarte.
    root: PathBuf,
    expected: HashMap<String, String>,
    files: BTreeMap<String, Vec<u8>>,
    total_bytes: usize,
    missing: BTreeSet<String>,
    skipped: BTreeSet<String>,
}

impl Collector {
    fn note_absent(&mut self, path: &str, need: Need) {
        match need {
            Need::Required => self.missing.insert(path.to_owned()),
            Need::Optional => self.skipped.insert(path.to_owned()),
        };
    }

    /// Prend un fichier ; `false` s'il manque.
    fn take(&mut self, path: &str, need: Need) -> AppResult<bool> {
        if self.files.contains_key(path) {
            return Ok(true);
        }
        let Some(content) = read_inside(&self.root, path)? else {
            self.note_absent(path, need);
            return Ok(false);
        };
        verify_optional_sha256(path, &content, &self.expected, false).map_err(invalid)?;
        self.total_bytes += content.len();
        if self.total_bytes > MAX_ITEM_BYTES || self.files.len() >= MAX_FILES {
            return Err(AppError::InvalidManifest(
                "élément trop volumineux".to_owned(),
            ));
        }
        self.files.insert(path.to_owned(), content);
        Ok(true)
    }

    /// Prend un fichier déclaré dans `folder` ; rend son chemin s'il est là.
    fn take_declared(
        &mut self,
        folder: &str,
        declared: &str,
        need: Need,
    ) -> AppResult<Option<String>> {
        let Some(path) = declared_path(folder, declared) else {
            // Hors du dossier : le moteur ne le lit pas non plus.
            self.note_absent(declared, Need::Optional);
            return Ok(None);
        };
        Ok(self.take(&path, need)?.then_some(path))
    }

    fn json(&self, path: &str) -> Option<Value> {
        serde_json::from_slice(self.files.get(path)?).ok()
    }

    /// Manifeste, fichiers déclarés, icône et contrat d'outil.
    fn take_item(&mut self, entry: &CatalogEntry) -> AppResult<()> {
        let manifest = safe_path(&entry.manifest_path).ok_or_else(|| {
            AppError::InvalidManifest(format!("{} : chemin de manifeste refusé", entry.id))
        })?;
        let folder = folder_of(&manifest);
        if self.take(&manifest, Need::Required)? {
            let document = self
                .json(&manifest)
                .ok_or_else(|| AppError::InvalidManifest(format!("{manifest} : JSON invalide")))?;
            self.take_declared_files(folder, entry, &document)?;
            let icon = declared_icon_path(Path::new(""), &manifest, &document);
            if let Some(icon) = icon.as_deref().and_then(Path::to_str).and_then(safe_path) {
                self.take(&icon, Need::Optional)?;
            }
        }
        if entry.entry_type == CatalogItemType::App {
            self.take_distribution(folder, &entry.id)?;
        }
        Ok(())
    }

    fn take_declared_files(
        &mut self,
        folder: &str,
        entry: &CatalogEntry,
        manifest: &Value,
    ) -> AppResult<()> {
        let is_resource = entry.entry_type.section() == Some(CatalogSection::Other);
        let mut has_asset = false;
        let declared = manifest.get("files").and_then(Value::as_array);
        for file in declared.into_iter().flatten() {
            let Some(path) = file.get("path").and_then(Value::as_str) else {
                continue;
            };
            let role = file
                .get("role")
                .and_then(|role| DeclaredRole::deserialize(role).ok());
            let need = match role {
                Some(DeclaredRole::Docs) => continue,
                Some(DeclaredRole::Operation | DeclaredRole::CapabilityProbe) => Need::Required,
                // Une ressource est son premier fichier.
                Some(DeclaredRole::Asset) if is_resource && !has_asset => {
                    has_asset = true;
                    Need::Required
                }
                Some(DeclaredRole::Asset) | None => Need::Optional,
            };
            let Some(taken) = self.take_declared(folder, path, need)? else {
                continue;
            };
            if role == Some(DeclaredRole::Operation)
                && let Some(operation) = self.json(&taken)
                && let Some(entrypoint) = script_entrypoint(&operation)
            {
                self.take_declared(folder, entrypoint, Need::Required)?;
            }
        }
        Ok(())
    }

    /// Contrat d'un outil : à côté du manifeste, sinon dans `apps/<id>` (comme `read_distribution`).
    fn take_distribution(&mut self, folder: &str, tool_id: &str) -> AppResult<()> {
        let Some(beside) = declared_path(folder, DISTRIBUTION_FILE_NAME) else {
            self.note_absent(DISTRIBUTION_FILE_NAME, Need::Required);
            return Ok(());
        };
        if self.take(&beside, Need::Required)? {
            return Ok(());
        }
        let shared = declared_path(&format!("apps/{tool_id}"), DISTRIBUTION_FILE_NAME);
        if let Some(shared) = shared
            && self.take(&shared, Need::Optional)?
        {
            self.missing.remove(&beside);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::installed::test_support::{Repository, write};
    use serde_json::json;

    #[test]
    fn a_plugin_is_read_with_its_operations_code_and_blocks() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let content = read_item(repository.path(), "demo")?;
        let paths: Vec<&str> = content.files().keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "plugins/demo/blocks/run.json",
                "plugins/demo/manifest.json",
                "plugins/demo/operations/run.json",
                "plugins/demo/run.py",
            ]
        );
        assert_eq!(content.missing, Vec::<String>::new());
        assert_eq!(content.skipped, ["plugins/demo/deploy.sh"]);
        assert!(matches!(
            read_item(repository.path(), "absent"),
            Err(AppError::NotFound)
        ));
        for refused in ["demo.run", "pack", "../demo"] {
            let error = read_item(repository.path(), refused);
            assert!(matches!(error, Err(AppError::Invalid)), "{refused}");
        }
        Ok(())
    }

    #[test]
    fn what_the_engine_reads_is_reported_missing() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        std::fs::remove_file(repository.path().join("plugins/demo/run.py"))?;
        std::fs::remove_file(repository.path().join("plugins/demo/blocks/run.json"))?;
        let content = read_item(repository.path(), "demo")?;
        assert_eq!(
            content.missing,
            ["plugins/demo/blocks/run.json", "plugins/demo/run.py"]
        );
        Ok(())
    }

    #[test]
    fn the_fingerprint_follows_the_content_only() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let first = read_item(repository.path(), "demo")?.content_sha256;
        assert_eq!(read_item(repository.path(), "demo")?.content_sha256, first);

        // Une date d'index et un fichier de documentation n'en font pas partie.
        repository.edit_entry("demo", "updatedAt", json!("2026-11-22"))?;
        write(repository.path(), "plugins/demo/README.md", "nouveau")?;
        assert_eq!(read_item(repository.path(), "demo")?.content_sha256, first);

        write(repository.path(), "plugins/demo/run.py", "print(2)")?;
        let code = read_item(repository.path(), "demo")?.content_sha256;
        assert_ne!(code, first);
        repository.edit_entry("demo", "version", json!("1.1.0"))?;
        assert_ne!(read_item(repository.path(), "demo")?.content_sha256, code);
        Ok(())
    }

    #[test]
    fn the_fingerprint_ignores_the_order_of_index_fields() -> anyhow::Result<()> {
        let one: Value = serde_json::from_str(r#"{"id":"x","name":"X"}"#)?;
        let two: Value = serde_json::from_str(r#"{"name":"X","id":"x"}"#)?;
        let files = BTreeMap::new();
        assert_eq!(fingerprint(&[one], &files), fingerprint(&[two], &files));
        Ok(())
    }

    #[test]
    fn a_published_digest_is_checked() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        repository.edit_index(|index| {
            index["filesSha256"] = json!({"plugins/demo/run.py": sha256_hex("autre")});
        })?;
        assert!(matches!(
            read_item(repository.path(), "demo"),
            Err(AppError::InvalidManifest(_))
        ));
        Ok(())
    }

    #[test]
    fn unsafe_manifest_paths_are_refused_and_unsafe_files_never_copied() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        write(
            repository.path(),
            "plugins/demo/manifest.json",
            &json!({"files": [
                {"path": "../../../etc/passwd", "role": "operation"},
                {"path": "/etc/passwd", "role": "asset"}
            ]})
            .to_string(),
        )?;
        let content = read_item(repository.path(), "demo")?;
        assert_eq!(content.files().len(), 2, "manifeste et bloc");
        assert_eq!(content.skipped.len(), 2);

        for path in ["../manifest.json", "/etc/passwd", "index.json"] {
            repository.edit_entry("demo", "manifestPath", json!(path))?;
            let refused = read_item(repository.path(), "demo");
            assert!(
                matches!(refused, Err(AppError::InvalidManifest(_))),
                "{path}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_tool_needs_its_contract() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let contract = "apps/outil/distribution.v1.json";
        let content = read_item(repository.path(), "outil")?;
        assert_eq!(content.missing, ["apps/outil/distribution.v1.json"]);
        write(repository.path(), contract, "{}")?;
        let content = read_item(repository.path(), "outil")?;
        assert!(content.missing.is_empty() && content.files().contains_key(contract));

        // Fiche rangée ailleurs : le contrat reste dans `apps/<id>`.
        write(repository.path(), "outils/manifest.json", "{}")?;
        repository.edit_entry("outil", "manifestPath", json!("outils/manifest.json"))?;
        let content = read_item(repository.path(), "outil")?;
        assert!(content.missing.is_empty() && content.skipped.is_empty());
        assert!(content.files().contains_key(contract));
        Ok(())
    }

    #[test]
    fn a_declared_icon_is_copied() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let manifest = json!({"icon": " ./icon.svg "}).to_string();
        write(repository.path(), "plugins/demo/manifest.json", &manifest)?;
        let content = read_item(repository.path(), "demo")?;
        assert_eq!(content.skipped, ["plugins/demo/icon.svg"]);
        write(repository.path(), "plugins/demo/icon.svg", "<svg/>")?;
        let content = read_item(repository.path(), "demo")?;
        assert!(content.files().contains_key("plugins/demo/icon.svg"));
        Ok(())
    }

    #[test]
    fn copies_are_compared_relatively_to_their_manifest() -> anyhow::Result<()> {
        let repository = Repository::with_plugin()?;
        let original = read_item(repository.path(), "demo")?;
        let moved = Repository::with_plugin_in("scripts/demo")?;
        let copy = read_item(moved.path(), "demo")?;
        assert_ne!(copy.content_sha256, original.content_sha256);
        assert!(copy.has_same_files_as(&original));
        write(moved.path(), "scripts/demo/run.py", "print(2)")?;
        let modified = read_item(moved.path(), "demo")?;
        assert!(!modified.has_same_files_as(&original));
        Ok(())
    }
}
