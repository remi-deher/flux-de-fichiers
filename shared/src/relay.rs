//! Relais d'une tâche vers la file d'un autre worker (ADR 0018).

use crate::capabilities::CapabilityRequirement;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, time::Duration};
use uuid::Uuid;

/// Nombre maximal de relais d'une même tâche, pour éviter qu'elle ne circule indéfiniment.
pub const MAX_RELAYS_PER_JOB: i64 = 3;

/// Longueur maximale (en caractères) du message d'un nœud transmis au worker repreneur.
pub const MAX_RELAYED_MESSAGE_CHARS: usize = 2000;
/// Taille maximale des sorties de nœuds transmises avec un relais, une fois sérialisées.
pub const MAX_RELAYED_OUTPUTS_BYTES: usize = 256 * 1024;

/// Ce qu'un nœud déjà exécuté expose aux suivants (`{{nodes.<id>.message|nextPort}}`), transmis au
/// worker repreneur. Le fichier de travail d'un nœud appartient au worker émetteur : il n'est pas transmis.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayedNodeOutput {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub next_port: Option<String>,
}

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
    /// Variables personnalisées du flux (`vars.*`) restaurées chez le repreneur.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub variables: Map<String, Value>,
    /// Sorties des nœuds exécutés avant le relais, pour que `{{nodes.<id>.…}}` reste lisible à la reprise.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub node_outputs: HashMap<String, RelayedNodeOutput>,
}

/// Garde d'âge d'un fichier de travail retenu : avant ce délai, son détenteur ne demande pas au
/// central s'il doit le purger.
pub const HANDOFF_TTL: Duration = Duration::from_secs(600);

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

/// Relais rapporté par un worker, avec les preuves de son bail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayReport {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    pub request: RelayRequest,
    /// Avertissement accumulé par le worker avant le relais.
    #[serde(default)]
    pub warning: bool,
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
    fn relay_report_warning_defaults_to_false_for_older_senders() -> Result<(), serde_json::Error> {
        let mut value = serde_json::json!({
            "worker_id": Uuid::nil(), "lease_token": Uuid::nil(), "lease_epoch": 1,
            "request": {"target": {"mode":"worker", "worker_id": Uuid::nil()}, "resumeNodeId":"next"}
        });
        let report: RelayReport = serde_json::from_value(value.clone())?;
        assert!(!report.warning);
        assert!(
            report.request.variables.is_empty(),
            "variables absentes des anciens workers"
        );
        value["warning"] = serde_json::json!(true);
        let report: RelayReport = serde_json::from_value(value)?;
        assert!(report.warning);
        Ok(())
    }

    #[test]
    fn relay_request_carries_flow_variables() -> Result<(), serde_json::Error> {
        let mut request = RelayRequest {
            target: RelayTarget::Worker {
                worker_id: Uuid::nil(),
            },
            resume_node_id: "next".into(),
            variables: Map::new(),
            node_outputs: HashMap::new(),
        };
        assert!(serde_json::to_value(&request)?.get("variables").is_none());
        request
            .variables
            .insert("codec".into(), Value::String("hevc".into()));
        let decoded: RelayRequest = serde_json::from_value(serde_json::to_value(&request)?)?;
        assert_eq!(decoded, request);
        Ok(())
    }
}
