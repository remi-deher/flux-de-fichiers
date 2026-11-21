//! Regroupement des pièces du catalogue par sous-catégorie officielle (ADR 0023).

use forgearr_shared::plugins::CatalogEntry;
use forgearr_shared::plugins::subcategory::{SubcategoryId, SubcategoryRegistry};

/// Langue des libellés de sous-catégories dans l'interface.
const CATALOG_LANGUAGE: &str = "fr";

/// Pièces d'une même sous-catégorie, sous le libellé officiel.
pub struct SubcategoryGroup {
    pub id: SubcategoryId,
    pub title: &'static str,
    order: u32,
    pub entries: Vec<CatalogEntry>,
}

/// Pièces sans sous-catégorie reconnue, puis groupes dans l'ordre du registre.
#[derive(Default)]
pub struct GroupedEntries {
    pub ungrouped: Vec<CatalogEntry>,
    pub groups: Vec<SubcategoryGroup>,
}

pub fn group_by_subcategory(entries: Vec<CatalogEntry>) -> GroupedEntries {
    let registry = SubcategoryRegistry::embedded();
    let mut grouped = GroupedEntries::default();
    for entry in entries {
        let Some(def) = entry.subcategory.as_ref().and_then(|id| registry.get(id)) else {
            grouped.ungrouped.push(entry);
            continue;
        };
        match grouped.groups.iter_mut().find(|group| group.id == def.id) {
            Some(group) => group.entries.push(entry),
            None => grouped.groups.push(SubcategoryGroup {
                id: def.id.clone(),
                title: def.label(CATALOG_LANGUAGE),
                order: def.order,
                entries: vec![entry],
            }),
        }
    }
    grouped
        .groups
        .sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::vocab::CatalogItemType;

    fn entry(id: &str, subcategory: Option<&str>) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0".to_owned(),
            name: id.to_owned(),
            manifest_path: format!("{id}.json"),
            description: None,
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            subcategory: subcategory.and_then(|raw| SubcategoryId::parse(raw).ok()),
            has_icon: false,
            tags: Vec::new(),
            owner: None,
            sha256: None,
            updated_at: None,
            runtime: None,
        }
    }

    fn ids(entries: &[CatalogEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.id.as_str()).collect()
    }

    #[test]
    fn groups_follow_the_registry_order_and_keep_entry_order() {
        let grouped = group_by_subcategory(vec![
            entry("notify", Some("notifications")),
            entry("sonarr", Some("media-managers")),
            entry("plex", Some("media-servers")),
            entry("bare", None),
            entry("jellyfin", Some("media-servers")),
        ]);
        assert_eq!(ids(&grouped.ungrouped), vec!["bare"]);
        let titles: Vec<&str> = grouped.groups.iter().map(|group| group.title).collect();
        assert_eq!(
            titles,
            vec![
                "Serveurs média",
                "Gestionnaires de médiathèque",
                "Notifications"
            ]
        );
        let media_servers = grouped.groups.first().map(|group| ids(&group.entries));
        assert_eq!(media_servers, Some(vec!["plex", "jellyfin"]));
    }

    #[test]
    fn encoding_and_subtitles_groups_sit_between_managers_and_notifications() {
        let grouped = group_by_subcategory(vec![
            entry("notify", Some("notifications")),
            entry("sous-titres", Some("media-subtitles")),
            entry("video", Some("media-encoding")),
            entry("vmaf", Some("media-encoding")),
            entry("sonarr", Some("media-managers")),
        ]);
        let titles: Vec<&str> = grouped.groups.iter().map(|group| group.title).collect();
        assert_eq!(
            titles,
            vec![
                "Gestionnaires de médiathèque",
                "Encodage et conversion",
                "Sous-titres",
                "Notifications"
            ]
        );
        let encoding = grouped.groups.get(1).map(|group| ids(&group.entries));
        assert_eq!(encoding, Some(vec!["video", "vmaf"]));
    }

    #[test]
    fn core_blocks_come_first_and_scripts_and_archives_last() {
        let grouped = group_by_subcategory(vec![
            entry("scripts", Some("scripting")),
            entry("archives", Some("archives")),
            entry("plex", Some("media-servers")),
            entry("essentiel", Some("core-blocks")),
        ]);
        let titles: Vec<&str> = grouped.groups.iter().map(|group| group.title).collect();
        assert_eq!(
            titles,
            vec!["Blocs essentiels", "Serveurs média", "Archives", "Scripts"]
        );
    }

    #[test]
    fn unknown_subcategory_ids_stay_ungrouped() {
        let grouped = group_by_subcategory(vec![entry("odd", Some("studio")), entry("none", None)]);
        assert_eq!(ids(&grouped.ungrouped), vec!["odd", "none"]);
        assert!(grouped.groups.is_empty());
    }
}
