//! Cartes de l'écran de départ : squelettes, modèles installés du catalogue et modèles locaux.

use super::skeleton::Skeleton;
use crate::ui::file_types_input::ALL_FILES;
use forgearr_shared::local::LocalFlowTemplate;
use forgearr_shared::plugins::store::{InstallState, ItemRef, StoreEntry};
use forgearr_shared::plugins::vocab::CatalogItemType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterGroup {
    Skeletons,
    Catalog,
    Store,
    Local,
}

impl StarterGroup {
    /// Ordre d'affichage.
    pub const ALL: [Self; 4] = [Self::Skeletons, Self::Catalog, Self::Store, Self::Local];

    pub fn title(self) -> &'static str {
        match self {
            Self::Skeletons => "Squelettes",
            Self::Catalog => "Modèles installés",
            Self::Store => "Modèles du Store à installer",
            Self::Local => "Mes modèles",
        }
    }
}

/// Ce que choisit l'utilisateur : un squelette, un modèle installé par son identifiant, ou un
/// modèle du Store qu'il faut d'abord installer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StarterKey {
    Skeleton(Skeleton),
    Template(String),
    Store(ItemRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StarterCard {
    pub key: StarterKey,
    pub group: StarterGroup,
    pub title: String,
    pub description: String,
    /// Types de fichiers proposés au départ (`*.mkv, *.mp4`).
    pub file_types: String,
}

/// Une carte par squelette ; ils n'imposent aucun type de fichiers.
pub fn skeleton_cards() -> Vec<StarterCard> {
    Skeleton::ALL
        .into_iter()
        .map(|skeleton| StarterCard {
            key: StarterKey::Skeleton(skeleton),
            group: StarterGroup::Skeletons,
            title: skeleton.label().to_owned(),
            description: skeleton.description().to_owned(),
            file_types: ALL_FILES.to_owned(),
        })
        .collect()
}

/// Une carte par modèle, rangée selon son origine.
pub fn template_cards(templates: &[LocalFlowTemplate]) -> Vec<StarterCard> {
    templates
        .iter()
        .map(|template| StarterCard {
            key: StarterKey::Template(template.id.clone()),
            group: if template.is_local {
                StarterGroup::Local
            } else {
                StarterGroup::Catalog
            },
            title: template.name.clone(),
            description: template
                .description
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or(&template.file_mask)
                .to_owned(),
            file_types: template.file_mask.clone(),
        })
        .collect()
}

/// Une carte par flux du Store pas encore installé ; les autres éléments et états sont ignorés.
pub fn store_cards(entries: &[StoreEntry]) -> Vec<StarterCard> {
    entries
        .iter()
        .filter(|entry| {
            entry.entry.entry_type == CatalogItemType::Flow
                && entry.install == InstallState::Available
        })
        .map(|entry| StarterCard {
            key: StarterKey::Store(ItemRef {
                repository_id: entry.origin.repository_id.clone(),
                id: entry.entry.id.clone(),
            }),
            group: StarterGroup::Store,
            title: entry.entry.name.clone(),
            description: entry
                .entry
                .description
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or(&entry.origin.repository_name)
                .to_owned(),
            file_types: ALL_FILES.to_owned(),
        })
        .collect()
}

/// Cartes regroupées dans l'ordre d'affichage ; un groupe vide n'apparaît pas.
pub fn group_cards(cards: &[StarterCard]) -> Vec<(StarterGroup, Vec<StarterCard>)> {
    StarterGroup::ALL
        .into_iter()
        .map(|group| {
            let members: Vec<StarterCard> = cards
                .iter()
                .filter(|card| card.group == group)
                .cloned()
                .collect();
            (group, members)
        })
        .filter(|(_, members)| !members.is_empty())
        .collect()
}

/// Nom proposé au flux créé depuis cette carte.
pub fn default_name(card: &StarterCard) -> String {
    match &card.key {
        StarterKey::Skeleton(skeleton) => skeleton.default_name().to_owned(),
        StarterKey::Template(_) | StarterKey::Store(_) => card.title.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template(id: &str, description: Option<&str>, is_local: bool) -> LocalFlowTemplate {
        LocalFlowTemplate {
            id: id.into(),
            name: format!("Modèle {id}"),
            description: description.map(str::to_owned),
            file_mask: "*.mkv, *.mp4".into(),
            version: "1.0.0".into(),
            definition: json!({}),
            is_local,
        }
    }

    /// Entrées de Store `(type, identifiant)`.
    fn store_entries(items: &[(&str, &str)]) -> Vec<StoreEntry> {
        items
            .iter()
            .filter_map(|(kind, id)| {
                serde_json::from_value(json!({
                    "origin": {
                        "repositoryId": "officiel",
                        "repositoryUrl": "https://example.test/depot",
                        "repositoryName": "Dépôt officiel"
                    },
                    "id": id,
                    "type": kind,
                    "version": "1.0.0",
                    "name": format!("Flux {id}"),
                    "manifestPath": format!("flows/{id}/manifest.json"),
                    "description": "Convertit en MKV"
                }))
                .ok()
            })
            .collect()
    }

    #[test]
    fn store_cards_keep_only_flows_that_are_not_installed_yet() {
        let mut entries = store_entries(&[
            ("flow", "remux"),
            ("plugin", "video"),
            ("flow", "installe"),
        ]);
        assert_eq!(entries.len(), 3);
        entries[2].install = InstallState::Installed {
            version: "1.0.0".into(),
            update: None,
            damaged: false,
        };
        let cards = store_cards(&entries);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].group, StarterGroup::Store);
        assert_eq!(cards[0].title, "Flux remux");
        assert_eq!(cards[0].description, "Convertit en MKV");
        assert_eq!(
            cards[0].key,
            StarterKey::Store(ItemRef {
                repository_id: "officiel".into(),
                id: "remux".into()
            })
        );
    }

    #[test]
    fn there_is_one_skeleton_card_per_skeleton() {
        let cards = skeleton_cards();
        assert_eq!(cards.len(), Skeleton::ALL.len());
        assert!(cards.iter().all(|card| card.file_types == ALL_FILES));
    }

    #[test]
    fn template_cards_take_the_origin_group_and_the_default_file_types() {
        let cards = template_cards(&[
            template("a", Some("Convertit"), false),
            template("b", None, true),
        ]);
        assert_eq!(cards[0].group, StarterGroup::Catalog);
        assert_eq!(cards[1].group, StarterGroup::Local);
        assert_eq!(cards[0].file_types, "*.mkv, *.mp4");
        assert_eq!(cards[0].description, "Convertit");
    }

    #[test]
    fn a_blank_description_falls_back_to_the_file_types() {
        let cards = template_cards(&[template("a", Some("  "), false)]);
        assert_eq!(cards[0].description, "*.mkv, *.mp4");
    }

    #[test]
    fn groups_follow_the_display_order_and_skip_empty_ones() {
        let mut cards = template_cards(&[template("l", None, true)]);
        cards.extend(skeleton_cards());
        let groups = group_cards(&cards);
        let order: Vec<StarterGroup> = groups.iter().map(|(group, _)| *group).collect();
        assert_eq!(order, vec![StarterGroup::Skeletons, StarterGroup::Local]);
    }

    #[test]
    fn default_name_is_the_skeleton_name_or_the_template_title() {
        let skeletons = skeleton_cards();
        assert_eq!(default_name(&skeletons[0]), "Nouveau flux");
        let templates = template_cards(&[template("a", None, false)]);
        assert_eq!(default_name(&templates[0]), "Modèle a");
    }

    #[test]
    fn groups_round_trip_through_serde() {
        for group in StarterGroup::ALL {
            let text = serde_json::to_string(&group).unwrap_or_default();
            assert_eq!(
                serde_json::from_str::<StarterGroup>(&text).ok(),
                Some(group)
            );
        }
    }
}
