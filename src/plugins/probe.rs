//! Contrat de sonde obligatoire. L'identité vient du catalogue, la présence du cache local.

use super::{CatalogCapability, CatalogEntry, CatalogItemType};
use crate::tools_cache::{ToolCacheManager, host_arch};

/// Outil que le catalogue désigne comme sonde courante.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeContract {
    pub tool_id: String,
    pub version: String,
}

/// Sonde courante du catalogue : un outil de capacité `probe` marqué choix actuel.
pub fn required_probe(entries: &[CatalogEntry]) -> Option<ProbeContract> {
    entries
        .iter()
        .find(|entry| {
            entry.entry_type == CatalogItemType::App
                && entry.capability == Some(CatalogCapability::Probe)
                && entry.current_choice == Some(true)
        })
        .map(|entry| ProbeContract {
            tool_id: entry.id.clone(),
            version: entry.version.clone(),
        })
}

/// Le cache local a installé cette sonde. Un binaire du système ne compte pas.
pub fn probe_is_ready(cache: &ToolCacheManager, contract: &ProbeContract) -> bool {
    cache.is_ready(&contract.tool_id, &contract.version, host_arch())
}

/// Un job qui touche un fichier ne démarre que si le catalogue désigne une sonde
/// et que le cache local l'a installée.
pub fn file_job_may_start(cache: &ToolCacheManager, contract: Option<&ProbeContract>) -> bool {
    contract.is_some_and(|contract| probe_is_ready(cache, contract))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::vocab::CatalogItemType;

    fn tool(id: &str, version: &str, capability: CatalogCapability, current: bool) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            entry_type: CatalogItemType::App,
            version: version.to_owned(),
            name: id.to_owned(),
            manifest_path: format!("{id}.json"),
            capability: Some(capability),
            current_choice: Some(current),
            distribution: None,
            category: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn required_probe_is_the_current_probe_tool() -> Result<(), &'static str> {
        let entries = vec![
            tool("app.mkvtoolnix", "1.0.0", CatalogCapability::Remux, true),
            tool("app.mediainfo", "1.0.0", CatalogCapability::Probe, true),
            tool("app.other-probe", "2.0.0", CatalogCapability::Probe, false),
        ];
        let probe = required_probe(&entries).ok_or("sonde absente")?;
        assert_eq!(probe.tool_id, "app.mediainfo");
        assert_eq!(probe.version, "1.0.0");
        Ok(())
    }

    #[test]
    fn required_probe_is_absent_without_a_current_choice() {
        let entries = vec![tool(
            "app.mediainfo",
            "1.0.0",
            CatalogCapability::Probe,
            false,
        )];
        assert!(required_probe(&entries).is_none());
    }

    #[test]
    fn probe_is_ready_only_when_the_cache_marker_exists() -> Result<(), std::io::Error> {
        let root = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(root.path());
        let contract = ProbeContract {
            tool_id: "app.mediainfo".into(),
            version: "1.0.0".into(),
        };
        assert!(!probe_is_ready(&cache, &contract));
        assert!(!file_job_may_start(&cache, Some(&contract)));
        assert!(!file_job_may_start(&cache, None));
        let dir = cache.tool_dir(&contract.tool_id, &contract.version, host_arch());
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(".ready"), b"")?;
        assert!(probe_is_ready(&cache, &contract));
        assert!(file_job_may_start(&cache, Some(&contract)));
        Ok(())
    }
}
