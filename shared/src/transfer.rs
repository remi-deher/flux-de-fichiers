//! Transfert direct d'une source entre workers (ADR 0019).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}
