//! Contrat du flux d'événements en temps réel `GET /api/v1/events` (ADR 0028).
//!
//! Un événement « changed » ne porte aucune donnée métier : il dit seulement quoi relire.

use crate::Permission;
use serde::{Deserialize, Serialize};

/// Flux unique de l'interface (tâches et changements) ; central seulement.
pub const EVENTS_PATH: &str = "/api/v1/events";
/// Nom SSE d'un changement ; sa charge est un [`ChangedEvent`].
pub const CHANGED_EVENT: &str = "changed";
/// Nom SSE demandant de tout relire une fois : des événements ont été manqués.
pub const RESYNC_EVENT: &str = "resync";
/// Droit de recevoir les événements de tâches.
pub const JOB_EVENTS_PERMISSION: Permission = Permission::ViewJobs;

/// Sujet dont le contenu a changé sur le central.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeTopic {
    /// Catalogue, librairie locale, blocs et variables exposées.
    Catalog,
    /// Applications distribuées : installation en cours, terminée, purge.
    Tools,
    /// Dépôts de plugins : liste, activation, synchronisation.
    Repositories,
    /// Réglages enregistrés des plugins.
    Settings,
}

impl ChangeTopic {
    pub const ALL: [Self; 4] = [
        Self::Catalog,
        Self::Tools,
        Self::Repositories,
        Self::Settings,
    ];

    /// Droits qui ouvrent la lecture du sujet : un seul suffit, comme sur les routes de consultation.
    pub const fn viewers(self) -> &'static [Permission] {
        match self {
            // `/api/v1/plugins`, `/api/v1/local` et `/api/v1/flows/blocks` ont chacun leur droit.
            Self::Catalog => &[
                Permission::ViewPlugins,
                Permission::ViewLibraries,
                Permission::ViewFlows,
            ],
            Self::Tools | Self::Repositories => &[Permission::ViewPlugins],
            Self::Settings => &[Permission::ManagePlugins],
        }
    }
}

/// Charge JSON d'un événement « changed ».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedEvent {
    pub topic: ChangeTopic,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;

    #[test]
    fn a_changed_event_carries_only_its_topic() -> Result<(), serde_json::Error> {
        for (topic, name) in
            ChangeTopic::ALL
                .into_iter()
                .zip(["catalog", "tools", "repositories", "settings"])
        {
            let json = serde_json::to_string(&ChangedEvent { topic })?;
            assert_eq!(json, format!(r#"{{"topic":"{name}"}}"#));
            assert_eq!(serde_json::from_str::<ChangedEvent>(&json)?.topic, topic);
        }
        // Un sujet d'un central plus récent est refusé : le client l'ignore.
        assert!(serde_json::from_str::<ChangedEvent>(r#"{"topic":"inconnu"}"#).is_err());
        Ok(())
    }

    #[test]
    fn only_those_who_can_read_a_topic_receive_it() {
        let received = |role: Role| {
            ChangeTopic::ALL
                .into_iter()
                .filter(|topic| topic.viewers().iter().any(|right| role.allows(*right)))
                .collect::<Vec<_>>()
        };
        assert_eq!(received(Role::Admin), ChangeTopic::ALL);
        assert_eq!(
            received(Role::Viewer),
            [
                ChangeTopic::Catalog,
                ChangeTopic::Tools,
                ChangeTopic::Repositories
            ]
        );
        assert!(received(Role::Worker).is_empty());
    }
}
