//! Sous-catégories déclarées par les dépôts, validées par le registre officiel (ADR 0023).
//!
//! Une valeur non retenue ne casse jamais le catalogue : le plugin reste dans sa catégorie,
//! et l'auteur du dépôt en est averti (journal à la synchronisation, écran du dépôt).

use super::{CatalogEntry, vocab};
use forgearr_shared::plugins::subcategory::SubcategoryRegistry;
use serde_json::Value;

/// Retient la sous-catégorie d'une entrée si le registre la valide pour sa catégorie ;
/// sinon repli silencieux (aucune sous-catégorie). La valeur du manifeste gagne sur celle de l'index.
pub fn resolve_entry_subcategory(entry: &mut CatalogEntry, declared: Option<&str>) {
    let from_index = entry.subcategory.take().map(|id| id.to_string());
    let raw = declared.map(str::to_owned).or(from_index);
    entry.subcategory = raw.and_then(|raw| {
        SubcategoryRegistry::embedded()
            .check(&raw, entry.category)
            .ok()
            .map(|def| def.id.clone())
    });
}

/// Avertissement pour l'auteur si le document déclare une sous-catégorie non retenue.
pub fn document_warning(
    label: &str,
    document: &Value,
    index_category: Option<vocab::PluginCategory>,
) -> Option<String> {
    let labels = vocab::labels_from_document(document);
    let raw = labels.subcategory?;
    let category = labels.category.or(index_category);
    SubcategoryRegistry::embedded()
        .check(&raw, category)
        .err()
        .map(|issue| format!("{label} : sous-catégorie « {raw} » ignorée ({issue})"))
}

/// Avertissements d'un dépôt : un par entrée dont la sous-catégorie n'est pas retenue.
/// `read_document` fournit le texte du manifeste d'une entrée, s'il est disponible.
pub fn repository_warnings(
    entries: &[CatalogEntry],
    mut read_document: impl FnMut(&CatalogEntry) -> Option<String>,
) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let document: Value = serde_json::from_str(&read_document(entry)?).ok()?;
            document_warning(&entry.id, &document, entry.category)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{CatalogItemType, PluginCategory};
    use forgearr_shared::plugins::subcategory::SubcategoryId;
    use serde_json::json;

    fn entry(category: Option<PluginCategory>) -> CatalogEntry {
        CatalogEntry {
            id: "plugin.plex".into(),
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0".into(),
            name: "Plex".into(),
            manifest_path: "plugins/plex.json".into(),
            description: None,
            capability: None,
            current_choice: None,
            distribution: None,
            category,
            subcategory: None,
            tags: Vec::new(),
            owner: None,
            sha256: None,
            updated_at: None,
            runtime: None,
        }
    }

    fn media_servers() -> anyhow::Result<SubcategoryId> {
        Ok(SubcategoryId::parse("media-servers")?)
    }

    #[test]
    fn known_subcategory_under_its_category_is_kept() -> anyhow::Result<()> {
        let mut plex = entry(Some(PluginCategory::Media));
        resolve_entry_subcategory(&mut plex, Some("media-servers"));
        assert_eq!(plex.subcategory, Some(media_servers()?));
        Ok(())
    }

    #[test]
    fn unknown_malformed_or_misplaced_subcategory_falls_back_silently() {
        for raw in ["studio", "Media Servers", ""] {
            let mut plex = entry(Some(PluginCategory::Media));
            resolve_entry_subcategory(&mut plex, Some(raw));
            assert_eq!(plex.subcategory, None, "{raw}");
        }
        let mut other = entry(Some(PluginCategory::Io));
        resolve_entry_subcategory(&mut other, Some("media-servers"));
        assert_eq!(other.subcategory, None);
        let mut undeclared = entry(None);
        resolve_entry_subcategory(&mut undeclared, Some("media-servers"));
        assert_eq!(undeclared.subcategory, None);
    }

    #[test]
    fn index_value_is_revalidated_and_the_manifest_value_wins() -> anyhow::Result<()> {
        let mut from_index = entry(Some(PluginCategory::Media));
        from_index.subcategory = Some(media_servers()?);
        resolve_entry_subcategory(&mut from_index, None);
        assert_eq!(from_index.subcategory, Some(media_servers()?));

        let mut misplaced = entry(Some(PluginCategory::Io));
        misplaced.subcategory = Some(media_servers()?);
        resolve_entry_subcategory(&mut misplaced, None);
        assert_eq!(misplaced.subcategory, None);

        let mut overridden = entry(Some(PluginCategory::Media));
        overridden.subcategory = Some(SubcategoryId::parse("media-managers")?);
        resolve_entry_subcategory(&mut overridden, Some("media-servers"));
        assert_eq!(overridden.subcategory, Some(media_servers()?));
        Ok(())
    }

    #[test]
    fn warning_names_the_plugin_the_value_and_the_cause() {
        let document = json!({"category": "media", "subcategory": "studio"});
        let warning = document_warning("plugin.plex", &document, None);
        assert_eq!(
            warning.as_deref(),
            Some("plugin.plex : sous-catégorie « studio » ignorée (absente du registre officiel)")
        );

        let misplaced = json!({"category": "io", "subcategory": "media-servers"});
        let warning = document_warning("plugin.plex", &misplaced, None).unwrap_or_default();
        assert!(warning.contains("rattachée à la catégorie « media »"));
    }

    #[test]
    fn no_warning_for_a_valid_or_undeclared_subcategory() {
        let valid = json!({"category": "media", "subcategory": "media-servers"});
        assert_eq!(document_warning("p", &valid, None), None);
        let none = json!({"category": "media"});
        assert_eq!(document_warning("p", &none, None), None);
        let index_category = json!({"subcategory": "media-servers"});
        assert_eq!(
            document_warning("p", &index_category, Some(PluginCategory::Media)),
            None
        );
    }

    #[test]
    fn repository_warnings_report_one_line_per_faulty_entry() {
        let entries = vec![entry(Some(PluginCategory::Media))];
        let faulty_document = r#"{"subcategory": "studio"}"#;
        let faulty = repository_warnings(&entries, |_| Some(faulty_document.to_owned()));
        assert_eq!(faulty.len(), 1);
        let healthy_document = r#"{"subcategory": "media-servers"}"#;
        let healthy = repository_warnings(&entries, |_| Some(healthy_document.to_owned()));
        assert!(healthy.is_empty());
        let unreadable = repository_warnings(&entries, |_| None);
        assert!(unreadable.is_empty());
        let invalid_json = repository_warnings(&entries, |_| Some("{".to_owned()));
        assert!(invalid_json.is_empty());
    }
}
