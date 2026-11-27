//! Exécution confinée des processus externes (ADR 0034).
//!
//! Un processus lancé par le central ou par un worker reçoit un profil (`ConfinementProfile`) : les
//! dossiers qu'il lit et écrit, le réseau, des plafonds de ressources. Le confinement est appliqué par le
//! noyau (Landlock, seccomp, `setrlimit`) avec des appels système directs, sans dépendance de plus, par une
//! sous-commande de l'exécutable de Forgearr (`sandbox-exec`) qui s'applique le profil puis remplace son
//! processus par le programme demandé. Linux seulement, sur x86_64 et aarch64.

mod command;
mod exec;
mod landlock;
mod limits;
mod seccomp;
mod support;

pub use command::{SANDBOX_EXEC_COMMAND, confined_command, confined_command_with};
pub use exec::{EXIT_LAUNCH_FAILED, EXIT_SETUP_FAILED, run_sandbox_exec};
pub use support::detect;

use forgearr_shared::confinement::Enforcement;
use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfineError {
    #[error("usage : forgearr sandbox-exec --profile <json> -- <programme> [arguments]")]
    Usage,
    #[error("profil de confinement illisible : {0}")]
    Profile(#[from] serde_json::Error),
    #[error("le noyau ne sait pas appliquer {0}")]
    Unsupported(&'static str),
    #[error("chemin inutilisable dans le profil : {0}")]
    InvalidPath(PathBuf),
    #[error("{operation} a échoué : {source}")]
    Syscall {
        operation: &'static str,
        source: io::Error,
    },
    #[error("lancement impossible de « {program} » : {source}")]
    Launch { program: String, source: io::Error },
}

/// L'erreur du dernier appel système, avec l'opération qui l'a provoquée.
fn last_os_error(operation: &'static str) -> ConfineError {
    ConfineError::Syscall {
        operation,
        source: io::Error::last_os_error(),
    }
}

/// Conduite devant une restriction que le noyau ne sait pas appliquer : échec avec la raison, ou
/// avertissement sur l'erreur standard selon l'`Enforcement` du profil.
fn unsupported(enforcement: Enforcement, restriction: &'static str) -> Result<(), ConfineError> {
    match enforcement {
        Enforcement::Require => Err(ConfineError::Unsupported(restriction)),
        Enforcement::BestEffort => {
            eprintln!("sandbox-exec : {restriction} n'est pas disponible, restriction ignorée");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_required_restriction_that_is_missing_fails_with_its_name() {
        let result = unsupported(Enforcement::Require, "Landlock");
        assert!(matches!(result, Err(ConfineError::Unsupported("Landlock"))));
        let message = ConfineError::Unsupported("Landlock").to_string();
        assert_eq!(message, "le noyau ne sait pas appliquer Landlock");
    }

    #[test]
    fn a_best_effort_restriction_that_is_missing_is_skipped() {
        assert!(unsupported(Enforcement::BestEffort, "seccomp").is_ok());
    }

    #[test]
    fn the_last_os_error_names_the_operation() {
        let error = last_os_error("ouverture");
        assert!(error.to_string().starts_with("ouverture a échoué : "));
    }
}
