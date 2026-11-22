//! Cadences et délais du protocole worker ↔ central, nommés une seule fois pour le worker distant
//! et le worker intégré.

use std::time::Duration;

/// Intervalle entre deux battements de bail d'une tâche en cours (bien en dessous de la durée du bail).
#[cfg(not(test))]
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
#[cfg(test)]
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(10);
/// Attente entre deux demandes de tâche quand rien n'est à traiter.
pub(crate) const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Intervalle entre deux relevés des capacités logicielles installées.
pub(crate) const SOFTWARE_PROBE_REFRESH: Duration = Duration::from_secs(60);
/// Demandes de tâche à vide entre deux ré-enregistrements auprès du central.
pub(crate) const REREGISTRATION_EVERY_IDLE_POLLS: u32 = 30;
/// Délai de connexion au central.
pub(crate) const CENTRAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Délai d'une requête courte au central (battement, rapport d'étapes, échec, enregistrement, tâche).
pub(crate) const CENTRAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Délai d'une demande de distribution d'outil.
pub(crate) const TOOL_DISTRIBUTION_TIMEOUT: Duration = Duration::from_secs(15);
/// Délai d'un rapport de progression, sans conséquence s'il est perdu.
pub(crate) const PROGRESS_REPORT_TIMEOUT: Duration = Duration::from_secs(5);
/// Pause avant de retenter l'enregistrement auprès du central.
pub(crate) const REGISTRATION_RETRY_DELAY: Duration = Duration::from_secs(5);
/// Pause entre deux essais d'un appel sans réponse du central.
#[cfg(not(test))]
pub(crate) const CALL_RETRY_DELAY: Duration = Duration::from_secs(5);
#[cfg(test)]
pub(crate) const CALL_RETRY_DELAY: Duration = Duration::from_millis(10);
/// Marge ajoutée au délai d'injoignabilité avant d'abandonner un appel : le central a alors tranché.
pub(crate) const PATIENCE_MARGIN_SECONDS: i64 = 60;
/// Pannes internes du central tolérées pour un même appel.
pub(crate) const MAX_BROKEN_REPLIES: u32 = 5;
/// Attente d'une réponse à l'annonce du résultat avant de la répéter (elle rejoint la publication).
pub(crate) const RESULT_READY_ATTEMPT: Duration = Duration::from_secs(5 * 60);
/// Durée totale d'attente de la publication, au-delà de celle que le central s'accorde.
pub(crate) const RESULT_READY_OVERALL: Duration =
    Duration::from_secs(forgearr_shared::transfer::FILE_TRANSFER_TIMEOUT.as_secs() * 2);
/// Attente des processus et des sections critiques d'une tâche abandonnée hors arrêt du worker.
pub(crate) const ABANDONED_WORK_TIMEOUT: Duration =
    forgearr_shared::transfer::FILE_TRANSFER_TIMEOUT;
/// Arrêt propre : attente des processus d'une tâche, puis de la réponse à sa restitution.
pub(crate) const SHUTDOWN_WORK_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const RELEASE_TIMEOUT: Duration = Duration::from_secs(2);
/// Durée totale d'un arrêt propre, sous les 10 s laissées par Docker.
pub(crate) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(8);
/// Attente, par le détenteur d'une source, de la réponse du central à un rapport de publication.
pub(crate) const PUBLICATION_REPORT_PATIENCE: Duration = Duration::from_secs(2 * 60);
