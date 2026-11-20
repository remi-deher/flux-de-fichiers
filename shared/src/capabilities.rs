//! Capacités matérielles et logicielles d'un worker (ADR 0018 et 0020).

use serde::{Deserialize, Serialize};

/// Codec vidéo cible d'un encodage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    /// Accepte aussi `h265`, valeur du bloc d'encodage.
    #[serde(alias = "h265")]
    Hevc,
    Av1,
}

impl VideoCodec {
    /// Libellé lisible dans l'interface.
    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Hevc => "H.265",
            Self::Av1 => "AV1",
        }
    }
}

/// Accélérateur matériel d'encodage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodeAcceleration {
    /// Encodeur logiciel fourni par une application de plugin.
    Cpu,
    /// NVIDIA NVENC.
    Nvenc,
    /// VA-API (Intel et AMD sous Linux).
    Vaapi,
}

impl EncodeAcceleration {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Nvenc => "nvenc",
            Self::Vaapi => "vaapi",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Nvenc => "NVENC",
            Self::Vaapi => "VA-API",
        }
    }
}

/// Un codec encodable par un accélérateur donné.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareEncoder {
    pub codec: VideoCodec,
    pub acceleration: EncodeAcceleration,
}

impl HardwareEncoder {
    /// Libellé court, par exemple « H.265 · NVENC ».
    pub fn label(self) -> String {
        format!("{} · {}", self.codec.label(), self.acceleration.label())
    }
}

/// Capacités déclarées par un worker, relevées par ses sondes matérielles.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerCapabilities {
    /// Modèles de cartes graphiques détectés (inventaire PCI).
    #[serde(default)]
    pub gpu_models: Vec<String>,
    /// Encodeurs matériels utilisables, triés et sans doublon.
    #[serde(default)]
    pub hardware_encoders: Vec<HardwareEncoder>,
    /// Codecs pris en charge par les encodeurs logiciels fournis par les plugins.
    #[serde(default)]
    pub software_encoders: Vec<VideoCodec>,
}

impl WorkerCapabilities {
    /// Indique si le worker sait encoder `codec` avec un accélérateur matériel.
    pub fn can_encode_in_hardware(&self, codec: VideoCodec) -> bool {
        self.hardware_encoders.iter().any(|encoder| {
            encoder.codec == codec && encoder.acceleration != EncodeAcceleration::Cpu
        })
    }

    /// Trie et dédoublonne les encodeurs pour un rendu et une comparaison stables.
    pub fn normalize(&mut self) {
        self.hardware_encoders
            .retain(|encoder| encoder.acceleration != EncodeAcceleration::Cpu);
        self.hardware_encoders.sort();
        self.hardware_encoders.dedup();
        self.software_encoders.sort();
        self.software_encoders.dedup();
        self.gpu_models.sort();
        self.gpu_models.dedup();
    }
}

/// Capacité demandée par un bloc : un codec, avec ou sans accélérateur imposé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRequirement {
    pub codec: VideoCodec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceleration: Option<EncodeAcceleration>,
}

impl WorkerCapabilities {
    /// Indique si les encodeurs déclarés satisfont l'exigence.
    pub fn satisfies(&self, requirement: &CapabilityRequirement) -> bool {
        let software = requirement
            .acceleration
            .is_none_or(|acceleration| acceleration == EncodeAcceleration::Cpu)
            && self.software_encoders.contains(&requirement.codec);
        software
            || self.hardware_encoders.iter().any(|encoder| {
                encoder.codec == requirement.codec
                    && encoder.acceleration != EncodeAcceleration::Cpu
                    && requirement
                        .acceleration
                        .is_none_or(|acceleration| acceleration == encoder.acceleration)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_encoder_serialises_with_stable_names() -> Result<(), serde_json::Error> {
        let encoder = HardwareEncoder {
            codec: VideoCodec::Hevc,
            acceleration: EncodeAcceleration::Nvenc,
        };
        assert_eq!(
            serde_json::to_string(&encoder)?,
            r#"{"codec":"hevc","acceleration":"nvenc"}"#
        );
        assert_eq!(encoder.label(), "H.265 · NVENC");
        Ok(())
    }

    #[test]
    fn hevc_accepts_the_encode_block_spelling() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::from_str::<VideoCodec>(r#""h265""#)?,
            VideoCodec::Hevc
        );
        assert_eq!(serde_json::to_string(&VideoCodec::Hevc)?, r#""hevc""#);
        Ok(())
    }

    #[test]
    fn capabilities_default_when_absent_on_the_wire() -> Result<(), serde_json::Error> {
        let parsed: WorkerCapabilities = serde_json::from_str("{}")?;
        assert_eq!(parsed, WorkerCapabilities::default());
        assert!(!parsed.can_encode_in_hardware(VideoCodec::Hevc));
        Ok(())
    }

    #[test]
    fn normalize_sorts_and_deduplicates() {
        let vaapi_h264 = HardwareEncoder {
            codec: VideoCodec::H264,
            acceleration: EncodeAcceleration::Vaapi,
        };
        let nvenc_hevc = HardwareEncoder {
            codec: VideoCodec::Hevc,
            acceleration: EncodeAcceleration::Nvenc,
        };
        let mut capabilities = WorkerCapabilities {
            gpu_models: vec!["B".into(), "A".into(), "A".into()],
            hardware_encoders: vec![nvenc_hevc, vaapi_h264, nvenc_hevc],
            software_encoders: vec![VideoCodec::Av1, VideoCodec::H264, VideoCodec::Av1],
        };
        capabilities.normalize();
        assert_eq!(capabilities.hardware_encoders, [vaapi_h264, nvenc_hevc]);
        assert_eq!(capabilities.gpu_models, ["A", "B"]);
        assert_eq!(
            capabilities.software_encoders,
            [VideoCodec::H264, VideoCodec::Av1]
        );
        assert!(capabilities.can_encode_in_hardware(VideoCodec::Hevc));
        assert!(!capabilities.can_encode_in_hardware(VideoCodec::Av1));
    }

    #[test]
    fn requirement_matches_codec_and_optional_acceleration() {
        let capabilities = WorkerCapabilities {
            gpu_models: Vec::new(),
            hardware_encoders: vec![HardwareEncoder {
                codec: VideoCodec::Hevc,
                acceleration: EncodeAcceleration::Vaapi,
            }],
            software_encoders: vec![VideoCodec::Av1],
        };
        let any = CapabilityRequirement {
            codec: VideoCodec::Hevc,
            acceleration: None,
        };
        let nvenc = CapabilityRequirement {
            acceleration: Some(EncodeAcceleration::Nvenc),
            ..any
        };
        let av1 = CapabilityRequirement {
            codec: VideoCodec::Av1,
            ..any
        };
        assert!(capabilities.satisfies(&any));
        assert!(!capabilities.satisfies(&nvenc));
        assert!(capabilities.satisfies(&av1));
        assert!(capabilities.satisfies(&CapabilityRequirement {
            acceleration: Some(EncodeAcceleration::Cpu),
            ..av1
        }));
        assert!(!capabilities.satisfies(&CapabilityRequirement {
            acceleration: Some(EncodeAcceleration::Vaapi),
            ..av1
        }));
        assert!(!capabilities.satisfies(&CapabilityRequirement {
            acceleration: Some(EncodeAcceleration::Cpu),
            ..any
        }));
    }

    #[test]
    fn cpu_entry_in_the_hardware_list_does_not_prove_a_software_encoder() {
        let mut capabilities = WorkerCapabilities {
            hardware_encoders: vec![HardwareEncoder {
                codec: VideoCodec::Hevc,
                acceleration: EncodeAcceleration::Cpu,
            }],
            ..Default::default()
        };
        let requirement = CapabilityRequirement {
            codec: VideoCodec::Hevc,
            acceleration: Some(EncodeAcceleration::Cpu),
        };
        assert!(!capabilities.can_encode_in_hardware(VideoCodec::Hevc));
        assert!(!capabilities.satisfies(&requirement));
        assert!(!capabilities.satisfies(&CapabilityRequirement {
            acceleration: None,
            ..requirement
        }));
        capabilities.normalize();
        assert!(capabilities.hardware_encoders.is_empty());
    }
}
