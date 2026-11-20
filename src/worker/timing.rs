//! Cadences et délais du protocole worker ↔ central, nommés une seule fois pour le worker distant
//! et le worker intégré.

use std::time::Duration;

/// Intervalle entre deux battements de bail d'une tâche en cours (bien en dessous de la durée du bail).
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
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
