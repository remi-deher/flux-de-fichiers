//! Transfert direct d'une source entre workers (ADR 0019).

use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Durée maximale d'un transfert de fichier vers ou depuis le central (une heure) : résultat,
/// fichier de travail, requêtes HTTP du worker.
pub const FILE_TRANSFER_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// En-tête portant le billet de transfert sur la route de lecture du worker source.
pub const TRANSFER_TICKET_HEADER: &str = "x-transfer-ticket";

/// Durée de validité d'un billet : assez pour une reprise par `Range`, trop courte pour un rejeu utile.
pub const SOURCE_TICKET_TTL_SECONDS: i64 = 600;

/// Billet remis au worker traitant : où lire la source chez le worker qui la détient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTicket {
    /// Adresse du serveur de transfert du worker source.
    pub url: String,
    /// Secret opaque présenté au worker source ; seul son empreinte est stockée au central.
    pub token: String,
}

/// Demande du worker source au central : « ce billet est-il valide, et que dois-je servir ? ».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedeemRequest {
    pub token: String,
    pub worker_id: Uuid,
}

/// Ce que le worker source doit servir pour un billet valide.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferGrant {
    /// Racine du dossier de la source, telle que montée sur le worker source.
    pub root: String,
    /// Nom de la source sous cette racine.
    pub source_name: String,
    pub sha256: String,
    pub bytes: i64,
}

/// Demande du central à un worker source : « envoie-moi ce fichier » (repli, ADR 0019).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRelayTask {
    pub request_id: Uuid,
    pub worker_id: Uuid,
    pub root: String,
    pub source_name: String,
}

/// Publication du résultat sur un dossier détenu par un worker à accès direct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePublicationTask {
    pub request_id: Uuid,
    pub worker_id: Uuid,
    pub root: String,
    pub source_name: String,
    pub source_sha256: String,
    pub source_bytes: i64,
    pub result_sha256: String,
    pub result_bytes: i64,
    pub extension: String,
    pub policy: crate::libraries::LibraryOutputPolicy,
}

/// Résultat explicite du remplacement chez le détenteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePublicationOutcome {
    Published,
    SourceChanged,
    Failed,
}

/// Réponse du détenteur à une demande de publication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePublicationReport {
    pub request_id: Uuid,
    pub worker_id: Uuid,
    pub outcome: SourcePublicationOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_roundtrips_as_json() -> Result<(), serde_json::Error> {
        let ticket = SourceTicket {
            url: "http://nas:9000".into(),
            token: "abc".into(),
        };
        let json = serde_json::to_string(&ticket)?;
        assert_eq!(serde_json::from_str::<SourceTicket>(&json)?, ticket);
        Ok(())
    }

    #[test]
    fn publication_report_roundtrips_with_typed_outcomes() -> Result<(), serde_json::Error> {
        let report = SourcePublicationReport {
            request_id: Uuid::from_u128(1),
            worker_id: Uuid::from_u128(2),
            outcome: SourcePublicationOutcome::SourceChanged,
        };
        let json = serde_json::to_string(&report)?;
        assert!(json.contains("source_changed"));
        assert_eq!(
            serde_json::from_str::<SourcePublicationReport>(&json)?,
            report
        );
        Ok(())
    }
}
