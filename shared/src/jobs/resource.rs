//! Classe de ressource d'une tâche : la place (CPU ou GPU) qu'elle occupe sur un worker.
//!
//! La classe est déduite du flux figé à la création (voir `flows::flow_resource_class`) et
//! persistée dans `jobs.resource_class` ; `claim` la confronte aux places du worker.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Classe persistée par `jobs.resource_class` (CHECK SQL : cpu, gpu).
///
/// Une valeur inconnue se lit `Cpu` : c'est la classe sans exigence matérielle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", from = "String")]
pub enum JobResourceClass {
    #[default]
    Cpu,
    Gpu,
}

impl JobResourceClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
        }
    }

    /// Valeur stockée → classe ; toute valeur inconnue donne `Cpu`.
    pub fn parse_or_default(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case(Self::Gpu.as_str()) {
            Self::Gpu
        } else {
            Self::Cpu
        }
    }
}

impl From<String> for JobResourceClass {
    fn from(raw: String) -> Self {
        Self::parse_or_default(&raw)
    }
}

/// Classe de ressource d'une tâche listée (`GET /api/v1/jobs/resources`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResourceInfo {
    pub job_id: Uuid,
    pub resource_class: JobResourceClass,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_and_unknown_values_fall_back_to_cpu() -> Result<(), serde_json::Error> {
        for class in [JobResourceClass::Cpu, JobResourceClass::Gpu] {
            let json = serde_json::to_string(&class)?;
            assert_eq!(json, format!("\"{}\"", class.as_str()));
            assert_eq!(serde_json::from_str::<JobResourceClass>(&json)?, class);
        }
        assert_eq!(
            serde_json::from_str::<JobResourceClass>("\"tpu\"")?,
            JobResourceClass::Cpu
        );
        assert_eq!(JobResourceClass::parse_or_default("GPU"), JobResourceClass::Gpu);
        assert_eq!(JobResourceClass::parse_or_default(""), JobResourceClass::Cpu);
        Ok(())
    }
}
