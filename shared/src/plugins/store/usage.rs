//! Ce qui se sert d'un élément installé, et empêche donc de le désinstaller.

use serde::{Deserialize, Serialize};

/// Noms cités avant « et n autres ».
const NAMED_IN_SUMMARY: usize = 3;

/// Usages d'un élément installé ; les listes portent des noms affichables.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemUsage {
    pub flows: Vec<String>,
    /// Tâches non terminées, flux d'échec et notifications en attente compris.
    pub jobs: Vec<String>,
    /// Modèles de flux ou de bloc de la collection locale.
    pub templates: Vec<String>,
    /// Scripts de la collection locale.
    pub scripts: Vec<String>,
    /// Porte la dernière sonde primaire du moteur.
    pub last_primary_probe: bool,
    /// Porte la dernière opération par défaut du moteur.
    pub last_default_operation: bool,
}

impl ItemUsage {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Phrase qui nomme les usages ; vide s'il n'y en a pas.
    pub fn summary(&self) -> String {
        let lists = [
            ("flux", &self.flows),
            ("tâches", &self.jobs),
            ("modèles", &self.templates),
            ("scripts", &self.scripts),
        ];
        let mut parts: Vec<String> = lists
            .into_iter()
            .filter(|(_, names)| !names.is_empty())
            .map(|(label, names)| format!("{label} : {}", name_some(names)))
            .collect();
        if self.last_primary_probe {
            parts.push("dernière sonde primaire du moteur".to_owned());
        }
        if self.last_default_operation {
            parts.push("dernière opération par défaut du moteur".to_owned());
        }
        parts.join(" ; ")
    }
}

/// Les premiers noms, puis le nombre des autres.
pub fn name_some(names: &[String]) -> String {
    let named = names
        .iter()
        .take(NAMED_IN_SUMMARY)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match names.len().saturating_sub(NAMED_IN_SUMMARY) {
        0 => named,
        1 => format!("{named} et 1 autre"),
        others => format!("{named} et {others} autres"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(count: usize) -> Vec<String> {
        (1..=count).map(|index| format!("n{index}")).collect()
    }

    #[test]
    fn a_usage_names_what_holds_the_item() {
        assert!(ItemUsage::default().is_empty());
        assert_eq!(ItemUsage::default().summary(), "");
        let usage = ItemUsage {
            flows: names(5),
            scripts: names(1),
            last_primary_probe: true,
            ..ItemUsage::default()
        };
        assert!(!usage.is_empty());
        assert_eq!(
            usage.summary(),
            "flux : n1, n2, n3 et 2 autres ; scripts : n1 ; dernière sonde primaire du moteur"
        );
        assert_eq!(name_some(&names(4)), "n1, n2, n3 et 1 autre");
    }
}
