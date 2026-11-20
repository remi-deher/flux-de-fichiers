//! Relais d'une tâche vers la file d'un autre worker (ADR 0018).

use crate::capabilities::CapabilityRequirement;
use serde::{Deserialize, Serialize};
use std::time::Duration;
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

/// Durée maximale d'attente de la remise d'un fichier de travail.
pub const HANDOFF_TTL_SECONDS: i64 = 600;
/// [`HANDOFF_TTL_SECONDS`] en durée, pour les délais côté worker.
pub const HANDOFF_TTL: Duration = Duration::from_secs(HANDOFF_TTL_SECONDS as u64);

/// Fichier de travail conservé par le worker émetteur.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffArtifact {
    pub name: String,
    pub sha256: String,
    pub bytes: i64,
}

/// Fichier précédemment vérifié que le repreneur doit utiliser pour ce bail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffInput {
    pub source_lease_epoch: i64,
    pub artifact: HandoffArtifact,
}

/// Étape de la remise conservée par le central.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffState {
    Uploading,
    Pending,
    Claimed,
    Acknowledged,
    Failed,
}

impl HandoffState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "uploading",
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Acknowledged => "acknowledged",
            Self::Failed => "failed",
        }
    }
}

/// Offre temporaire remise au repreneur avant que le bail d'origine ne soit libéré.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffOffer {
    pub job_id: Uuid,
    pub from_worker_id: Uuid,
    pub source_lease_epoch: i64,
    pub artifact: HandoffArtifact,
    pub receipt_token: String,
}

/// Accusé de réception d'un fichier déjà téléchargé et vérifié par le repreneur.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffAcknowledgement {
    pub worker_id: Uuid,
    pub receipt_token: String,
    pub sha256: String,
    pub bytes: i64,
}

/// Demande initiale accompagnée du bail qui reste détenu par l'émetteur.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffBegin {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    pub lease_epoch: i64,
    pub request: RelayRequest,
    pub artifact: HandoffArtifact,
    /// Avertissement accumulé par les étapes déjà exécutées.
    #[serde(default)]
    pub warning: bool,
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

/// Marque « relayée » d'une tâche, pour l'interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRelayInfo {
    pub job_id: Uuid,
    /// Worker qui a relayé la tâche.
    pub from_worker_id: Uuid,
    pub relay_count: i64,
    /// Worker désigné ; absent quand la tâche est offerte à tous les workers capables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_worker_id: Option<Uuid>,
    /// Exigence de capacité de la cible « capable ».
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<CapabilityRequirement>,
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

    #[test]
    fn handoff_warning_defaults_to_false_for_older_senders() -> Result<(), serde_json::Error> {
        let mut value = serde_json::json!({
            "workerId": Uuid::nil(), "leaseToken": Uuid::nil(), "leaseEpoch": 1,
            "request": {"target": {"mode":"worker", "worker_id": Uuid::nil()}, "resumeNodeId":"next"},
            "artifact": {"name":"file.mkv", "sha256":"a".repeat(64), "bytes":5}
        });
        let begin: HandoffBegin = serde_json::from_value(value.clone())?;
        assert!(!begin.warning);
        value["warning"] = serde_json::json!(true);
        let begin: HandoffBegin = serde_json::from_value(value)?;
        assert!(begin.warning);
        Ok(())
    }
}
