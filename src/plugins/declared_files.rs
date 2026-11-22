//! Scripts et ressources déclarés dans `files[]` des manifestes, copiés avec leur dépôt. Le défaut
//! d'un de ces fichiers est signalé sans empêcher la synchronisation ; seules une empreinte fausse et
//! une panne passagère la refusent, pour garder la copie précédente.

use super::{
    SyncError, operation_declaration_issue,
    sync::{Fetched, MAX_FILES, resolve_manifest_relative, verify_optional_sha256},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Component, Path, PathBuf},
};

/// Rôles copiés tels quels ; les autres fichiers utiles sont du JSON ou des icônes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum CopiedRole {
    Script,
    Asset,
}

impl CopiedRole {
    /// Une ressource peut être binaire ; un script ou une image SVG est du texte.
    fn requires_utf8(self, path: &str) -> bool {
        self == Self::Script || path.ends_with(".svg")
    }
}

/// L'un des deux chemins est-il un dossier de l'autre, ou le même ?
fn overlaps(known: &str, path: &str) -> bool {
    Path::new(known).starts_with(path) || Path::new(path).starts_with(known)
}

/// Chemin sans `.` ni barre finale ; `None` s'il ne désigne aucun fichier.
fn plain_path(path: &str) -> Option<String> {
    let plain: PathBuf = Path::new(path)
        .components()
        .filter(|part| matches!(part, Component::Normal(_)))
        .collect();
    (!plain.as_os_str().is_empty()).then(|| plain.to_string_lossy().into_owned())
}

/// Scripts et ressources à copier en plus de `queued` (chemins relatifs à la racine du dépôt), dans
/// la limite de `room` fichiers.
pub(super) fn declared_files(
    queued: &HashSet<String>,
    manifests: &[(String, String)],
    room: usize,
    skipped: &mut Vec<String>,
) -> BTreeMap<String, CopiedRole> {
    let mut wanted: BTreeMap<String, CopiedRole> = BTreeMap::new();
    let mut beyond_limit = 0usize;
    for (manifest_rel, content) in manifests {
        let Ok(document) = serde_json::from_str::<serde_json::Value>(content) else {
            continue;
        };
        let Some(files) = document.get("files").and_then(|files| files.as_array()) else {
            continue;
        };
        for file in files {
            let Some(role) = file
                .get("role")
                .and_then(|role| CopiedRole::deserialize(role).ok())
            else {
                continue;
            };
            let Some(path) = file
                .get("path")
                .and_then(|path| path.as_str())
                .and_then(|path| resolve_manifest_relative(manifest_rel, path))
                .and_then(|path| plain_path(&path))
            else {
                continue;
            };
            if queued.contains(&path) || wanted.contains_key(&path) {
                continue;
            }
            if wanted.len() >= room {
                beyond_limit += 1;
                continue;
            }
            if queued
                .iter()
                .chain(wanted.keys())
                .any(|known| overlaps(known, &path))
            {
                skipped.push(format!("{path} : chemin déjà pris par un autre fichier"));
                continue;
            }
            wanted.insert(path, role);
        }
    }
    if beyond_limit > 0 {
        skipped.push(format!(
            "{beyond_limit} scripts ou ressources non copiés : limite de {MAX_FILES} fichiers"
        ));
    }
    wanted
}

/// Garde les fichiers téléchargés qui passent les contrôles ; les autres sont signalés.
pub(super) fn collect_declared(
    fetched: Vec<Fetched<Vec<u8>>>,
    roles: &BTreeMap<String, CopiedRole>,
    expected_sha256: &HashMap<String, String>,
    require_digest: bool,
    skipped: &mut Vec<String>,
) -> Result<Vec<(String, Vec<u8>)>, SyncError> {
    let mut files = Vec::new();
    for (relative, result) in fetched {
        let content = match result {
            Ok(Some(content)) => content,
            Ok(None) => {
                skipped.push(format!("{relative} : introuvable"));
                continue;
            }
            Err(error) if error.is_transient() => return Err(error),
            Err(error) => {
                skipped.push(format!("{relative} : {error}"));
                continue;
            }
        };
        // Mode immuable : un fichier sans empreinte n'est pas copié, sans bloquer tout le dépôt.
        if require_digest && !expected_sha256.contains_key(&relative) {
            skipped.push(format!(
                "{relative} : empreinte SHA-256 manquante dans l'index"
            ));
            continue;
        }
        verify_optional_sha256(&relative, &content, expected_sha256, require_digest)?;
        let text = std::str::from_utf8(&content).ok();
        let needs_text = roles
            .get(&relative)
            .is_some_and(|role| role.requires_utf8(&relative));
        if needs_text && text.is_none() {
            skipped.push(format!("{relative} : texte UTF-8 attendu"));
            continue;
        }
        // Aucun rôle ne contourne le contrôle des contrats d'opération (ADR 0025).
        if let Some(issue) = text.and_then(operation_declaration_issue) {
            skipped.push(format!("{relative} : déclarations refusées ({issue})"));
            continue;
        }
        files.push((relative, content));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools_cache::ToolCacheManager;

    const MANIFEST: &str = "scripts/demo/manifest.json";
    const SCRIPT: &str = "scripts/demo/run.py";
    const ASSET: &str = "scripts/demo/modele.bin";
    const BINARY: [u8; 4] = [0xff, 0xfe, 0x00, 0x80];

    fn manifest(files: serde_json::Value) -> Vec<(String, String)> {
        vec![(
            MANIFEST.to_owned(),
            serde_json::json!({ "files": files }).to_string(),
        )]
    }

    fn roles() -> BTreeMap<String, CopiedRole> {
        BTreeMap::from([
            (SCRIPT.to_owned(), CopiedRole::Script),
            (ASSET.to_owned(), CopiedRole::Asset),
        ])
    }

    fn received(path: &str, body: &[u8]) -> Fetched<Vec<u8>> {
        (path.to_owned(), Ok(Some(body.to_vec())))
    }

    #[test]
    fn only_safe_scripts_and_assets_are_wanted() {
        let queued = HashSet::from(["index.json".to_owned(), MANIFEST.to_owned()]);
        let manifests = manifest(serde_json::json!([
            {"path": "run.py", "role": "script"},
            {"path": "./modele.bin/", "role": "asset"},
            {"path": ".", "role": "asset"},
            {"path": "README.md", "role": "docs"},
            {"path": "manifest.json", "role": "asset"},
            {"path": "../../../etc/passwd", "role": "script"},
            {"path": "/etc/passwd", "role": "asset"},
            {"path": "run.py/enfant", "role": "asset"},
            {"role": "script"}
        ]));
        let mut skipped = Vec::new();
        assert_eq!(
            declared_files(&queued, &manifests, MAX_FILES, &mut skipped),
            roles()
        );
        // Le dossier du manifeste et un « enfant » de script ne sont pas des fichiers copiables.
        assert_eq!(skipped.len(), 2, "{skipped:?}");
        assert!(skipped.iter().any(|s| s.contains("run.py/enfant")));
    }

    #[test]
    fn files_beyond_the_limit_are_reported_without_failing() {
        let manifests = manifest(serde_json::json!([
            {"path": "a.py", "role": "script"},
            {"path": "b.py", "role": "script"},
            {"path": "c.py", "role": "script"}
        ]));
        let mut skipped = Vec::new();
        let wanted = declared_files(&HashSet::new(), &manifests, 1, &mut skipped);
        assert_eq!(wanted.len(), 1);
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].starts_with("2 "));
    }

    #[test]
    fn an_asset_may_be_binary_but_a_script_may_not() -> Result<(), SyncError> {
        let mut skipped = Vec::new();
        let files = collect_declared(
            vec![received(SCRIPT, &BINARY), received(ASSET, &BINARY)],
            &roles(),
            &HashMap::new(),
            false,
            &mut skipped,
        )?;
        assert_eq!(files, [(ASSET.to_owned(), BINARY.to_vec())]);
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains(SCRIPT));
        Ok(())
    }

    #[test]
    fn a_missing_or_oversized_file_does_not_fail_the_synchronisation() -> Result<(), SyncError> {
        let mut skipped = Vec::new();
        let files = collect_declared(
            vec![
                (SCRIPT.to_owned(), Ok(None)),
                (ASSET.to_owned(), Err(SyncError::TooLarge(ASSET.to_owned()))),
            ],
            &roles(),
            &HashMap::new(),
            false,
            &mut skipped,
        )?;
        assert!(files.is_empty());
        assert_eq!(skipped.len(), 2);
        Ok(())
    }

    #[test]
    fn only_a_passing_failure_refuses_the_synchronisation() {
        let http = |status: u16| SyncError::Http {
            status,
            url: SCRIPT.to_owned(),
        };
        let collect = |error: SyncError| {
            collect_declared(
                vec![(SCRIPT.to_owned(), Err(error))],
                &roles(),
                &HashMap::new(),
                false,
                &mut Vec::new(),
            )
        };
        for passing in [
            SyncError::Network("coupé".into()),
            http(408),
            http(429),
            http(503),
        ] {
            assert!(collect(passing).is_err());
        }
        for lasting in [http(403), http(410)] {
            assert!(collect(lasting).is_ok_and(|files| files.is_empty()));
        }
    }

    #[test]
    fn a_declared_digest_is_checked_and_a_wrong_one_refuses_the_repository() {
        let digest = ToolCacheManager::compute_sha256(b"print()");
        let expected = HashMap::from([(SCRIPT.to_owned(), digest)]);
        let mut skipped = Vec::new();
        let mut collect = |body: &[u8], expected: &HashMap<String, String>, strict: bool| {
            collect_declared(
                vec![received(SCRIPT, body)],
                &roles(),
                expected,
                strict,
                &mut skipped,
            )
        };
        assert!(collect(b"print()", &expected, true).is_ok_and(|files| files.len() == 1));
        assert!(matches!(
            collect(b"autre()", &expected, false),
            Err(SyncError::IntegrityMismatch(_))
        ));
        // Mode immuable sans empreinte : le fichier seul est écarté.
        assert!(collect(b"print()", &HashMap::new(), true).is_ok_and(|files| files.is_empty()));
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn an_invalid_operation_contract_cannot_hide_behind_an_asset() -> Result<(), SyncError> {
        let invalid = br#"{"providerId":"p","runtime":{},"exposes":{"namespace":"nodes","contractVersion":"1.0.0","variables":[]}}"#;
        let mut skipped = Vec::new();
        let files = collect_declared(
            vec![received(ASSET, invalid)],
            &roles(),
            &HashMap::new(),
            false,
            &mut skipped,
        )?;
        assert!(files.is_empty());
        assert!(skipped[0].contains("réservé"));
        Ok(())
    }
}
