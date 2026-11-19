//! Capacités matérielles d'un worker (ADR 0018) : codecs encodables par un accélérateur détecté.
//!
//! Elles décrivent le **matériel** uniquement. L'encodage logiciel dépend de l'outil FFmpeg
//! fourni par les plugins (ADR 0012) et n'est pas déduit ici.

use serde::{Deserialize, Serialize};

/// Codec vidéo cible d'un encodage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
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
    /// NVIDIA NVENC.
    Nvenc,
    /// VA-API (Intel et AMD sous Linux).
    Vaapi,
}

impl EncodeAcceleration {
    pub const fn label(self) -> &'static str {
        match self {
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
}

impl WorkerCapabilities {
    /// Indique si le worker sait encoder `codec` avec un accélérateur matériel.
    pub fn can_encode_in_hardware(&self, codec: VideoCodec) -> bool {
        self.hardware_encoders
            .iter()
            .any(|encoder| encoder.codec == codec)
    }

    /// Trie et dédoublonne les encodeurs pour un rendu et une comparaison stables.
    pub fn normalize(&mut self) {
        self.hardware_encoders.sort();
        self.hardware_encoders.dedup();
        self.gpu_models.sort();
        self.gpu_models.dedup();
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
        };
        capabilities.normalize();
        assert_eq!(capabilities.hardware_encoders, [vaapi_h264, nvenc_hevc]);
        assert_eq!(capabilities.gpu_models, ["A", "B"]);
        assert!(capabilities.can_encode_in_hardware(VideoCodec::Hevc));
        assert!(!capabilities.can_encode_in_hardware(VideoCodec::Av1));
    }
}
