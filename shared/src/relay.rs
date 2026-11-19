//! Relais d'une tâche vers la file d'un autre worker (ADR 0018).

use crate::capabilities::CapabilityRequirement;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Nombre maximal de relais d'une même tâche, pour éviter qu'elle ne circule indéfiniment.
pub const MAX_RELAYS_PER_JOB: i64 = 3;

/// Cible d'un relais : un worker capable (choisi par le central) ou un worker précis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RelayTarget {
    Capable { requirement: CapabilityRequirement },
    Worker { worker_id: Uuid },
}

/// Demande de relais produite par le bloc « Relayer la tâche » et envoyée au central.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayRequest {
    pub target: RelayTarget,
    /// Nœud du flux où le worker repreneur reprend l'exécution.
    pub resume_node_id: String,
}

/// Relais rapporté par un worker, avec les preuves de son bail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayReport {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    pub request: RelayRequest,
}

/// Réponse du central à une demande de relais.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayResponse {
    /// `true` : la tâche attend dans la file du worker cible ; `false` : elle a échoué.
    pub relayed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_worker_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::VideoCodec;

    #[test]
    fn relay_target_is_tagged_by_mode() -> Result<(), serde_json::Error> {
        let capable = RelayTarget::Capable {
            requirement: CapabilityRequirement {
                codec: VideoCodec::Hevc,
                acceleration: None,
            },
        };
        assert_eq!(
            serde_json::to_string(&capable)?,
            r#"{"mode":"capable","requirement":{"codec":"hevc"}}"#
        );
        let worker: RelayTarget = serde_json::from_str(
            r#"{"mode":"worker","worker_id":"00000000-0000-0000-0000-000000000001"}"#,
        )?;
        assert!(matches!(worker, RelayTarget::Worker { .. }));
        Ok(())
    }
}
