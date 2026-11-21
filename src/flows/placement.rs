//! Blocs de placement (ADR 0018) : « Capacité du worker » et « Relayer la tâche ».

use forgearr_shared::{
    capabilities::{CapabilityRequirement, EncodeAcceleration, VideoCodec, WorkerCapabilities},
    relay::{RelayRequest, RelayTarget},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Sortie du bloc « Capacité du worker » quand le worker courant satisfait l'exigence.
pub const PORT_CAPABLE: &str = "capable";
/// Sortie du bloc « Capacité du worker » quand il ne la satisfait pas.
pub const PORT_INCAPABLE: &str = "incapable";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RelayMode {
    #[default]
    Capable,
    Worker,
}

/// Configuration du bloc « Relayer la tâche ».
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RelayBlockConfig {
    mode: RelayMode,
    codec: Option<VideoCodec>,
    acceleration: Option<EncodeAcceleration>,
    worker_id: Option<Uuid>,
}

/// Choisit la sortie du bloc « Capacité du worker » d'après les capacités du worker courant.
pub fn evaluate_capability(
    config: &Value,
    capabilities: Option<&WorkerCapabilities>,
) -> Result<&'static str, String> {
    let requirement: CapabilityRequirement = serde_json::from_value(config.clone())
        .map_err(|_| "le bloc « Capacité du worker » exige un codec valide".to_owned())?;
    let capable = capabilities.is_some_and(|capabilities| capabilities.satisfies(&requirement));
    Ok(if capable {
        PORT_CAPABLE
    } else {
        PORT_INCAPABLE
    })
}

/// Construit la demande de relais du bloc « Relayer la tâche ».
pub fn build_relay_request(config: &Value, resume_node_id: &str) -> Result<RelayRequest, String> {
    let config: RelayBlockConfig = serde_json::from_value(config.clone())
        .map_err(|_| "configuration invalide du bloc « Relayer la tâche »".to_owned())?;
    let target = match config.mode {
        RelayMode::Capable => RelayTarget::Capable {
            requirement: CapabilityRequirement {
                codec: config
                    .codec
                    .ok_or("le relais vers un worker capable exige un codec")?,
                acceleration: config.acceleration,
            },
        },
        RelayMode::Worker => RelayTarget::Worker {
            worker_id: config
                .worker_id
                .ok_or("le relais vers un worker précis exige son identifiant")?,
        },
    };
    Ok(RelayRequest {
        target,
        resume_node_id: resume_node_id.to_owned(),
        variables: Default::default(),
        node_outputs: HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::capabilities::HardwareEncoder;
    use serde_json::json;

    fn hevc_nvenc() -> WorkerCapabilities {
        WorkerCapabilities {
            gpu_models: Vec::new(),
            software_encoders: Vec::new(),
            hardware_encoders: vec![HardwareEncoder {
                codec: VideoCodec::Hevc,
                acceleration: EncodeAcceleration::Nvenc,
            }],
        }
    }

    #[test]
    fn capability_block_routes_on_the_current_worker() -> Result<(), String> {
        let config = json!({"codec": "hevc"});
        assert_eq!(
            evaluate_capability(&config, Some(&hevc_nvenc()))?,
            PORT_CAPABLE
        );
        assert_eq!(evaluate_capability(&config, None)?, PORT_INCAPABLE);
        let av1 = json!({"codec": "av1"});
        assert_eq!(
            evaluate_capability(&av1, Some(&hevc_nvenc()))?,
            PORT_INCAPABLE
        );
        assert!(evaluate_capability(&json!({}), None).is_err());
        Ok(())
    }

    #[test]
    fn capability_block_can_select_a_software_encoder() -> Result<(), String> {
        let capabilities = WorkerCapabilities {
            software_encoders: vec![VideoCodec::Hevc],
            ..Default::default()
        };
        assert_eq!(
            evaluate_capability(
                &json!({"codec":"hevc", "acceleration":"cpu"}),
                Some(&capabilities)
            )?,
            PORT_CAPABLE
        );
        assert_eq!(
            evaluate_capability(
                &json!({"codec":"hevc", "acceleration":"nvenc"}),
                Some(&capabilities)
            )?,
            PORT_INCAPABLE
        );
        assert!(
            matches!(build_relay_request(&json!({"codec":"hevc", "acceleration":"cpu"}), "next")?.target, RelayTarget::Capable { requirement } if requirement.acceleration == Some(EncodeAcceleration::Cpu))
        );
        Ok(())
    }

    #[test]
    fn relay_to_a_capable_worker_needs_a_codec() -> Result<(), String> {
        let request =
            build_relay_request(&json!({"codec": "hevc", "acceleration": "nvenc"}), "n4")?;
        assert_eq!(request.resume_node_id, "n4");
        assert!(matches!(
            request.target,
            RelayTarget::Capable { requirement } if requirement.codec == VideoCodec::Hevc
                && requirement.acceleration == Some(EncodeAcceleration::Nvenc)
        ));
        assert!(build_relay_request(&json!({"mode": "capable"}), "n4").is_err());
        Ok(())
    }

    #[test]
    fn relay_to_a_precise_worker_needs_its_identifier() -> Result<(), String> {
        let id = Uuid::new_v4();
        let request =
            build_relay_request(&json!({"mode": "worker", "workerId": id.to_string()}), "n4")?;
        assert_eq!(request.target, RelayTarget::Worker { worker_id: id });
        assert!(build_relay_request(&json!({"mode": "worker"}), "n4").is_err());
        assert!(build_relay_request(&json!({"mode": "inconnu"}), "n4").is_err());
        Ok(())
    }
}
