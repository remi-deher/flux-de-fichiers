//! Construction de la commande qui lance un programme confiné (ADR 0034, point 5).
//!
//! La commande est celle de la sous-commande `sandbox-exec` de l'exécutable de Forgearr, suivie du profil
//! en JSON, d'un séparateur et du programme avec ses arguments. L'appelant règle ensuite l'environnement,
//! le dossier courant et les tubes comme pour n'importe quelle commande : `sandbox-exec` les transmet au
//! programme.

use super::ConfineError;
use forgearr_shared::confinement::ConfinementProfile;
use std::{ffi::OsStr, path::Path, process::Command};

/// Premier argument de l'exécutable de Forgearr qui demande un lancement confiné.
pub const SANDBOX_EXEC_COMMAND: &str = "sandbox-exec";
pub(super) const PROFILE_FLAG: &str = "--profile";
pub(super) const ARGUMENTS_SEPARATOR: &str = "--";

/// Commande confinée lancée par un exécutable de Forgearr donné (les essais passent celui du binaire).
/// `tokio::process::Command::from` en fait une commande asynchrone.
pub fn confined_command_with(
    executable: &Path,
    profile: &ConfinementProfile,
    program: impl AsRef<OsStr>,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<Command, ConfineError> {
    let profile_json = serde_json::to_string(profile)?;
    let mut command = Command::new(executable);
    command
        .arg(SANDBOX_EXEC_COMMAND)
        .arg(PROFILE_FLAG)
        .arg(profile_json)
        .arg(ARGUMENTS_SEPARATOR)
        .arg(program)
        .args(arguments);
    Ok(command)
}

/// Commande confinée lancée par l'exécutable en cours (Forgearr lui-même).
pub fn confined_command(
    profile: &ConfinementProfile,
    program: impl AsRef<OsStr>,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<Command, ConfineError> {
    let executable = std::env::current_exe().map_err(|source| ConfineError::Launch {
        program: "forgearr".to_owned(),
        source,
    })?;
    confined_command_with(&executable, profile, program, arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::confinement::NetworkAccess;

    fn arguments_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_command_carries_the_profile_then_the_program() -> anyhow::Result<()> {
        let profile = ConfinementProfile::default().with_network(NetworkAccess::Any);
        let launcher = Path::new("/opt/forgearr");
        let command = confined_command_with(launcher, &profile, "ty", ["server"])?;
        assert_eq!(command.get_program(), "/opt/forgearr");
        let arguments = arguments_of(&command);
        assert_eq!(arguments[0], SANDBOX_EXEC_COMMAND);
        assert_eq!(arguments[1], PROFILE_FLAG);
        assert_eq!(serde_json::from_str::<ConfinementProfile>(&arguments[2])?, profile);
        assert_eq!(&arguments[3..], [ARGUMENTS_SEPARATOR, "ty", "server"]);
        Ok(())
    }

    #[test]
    fn the_current_executable_is_the_default_launcher() -> anyhow::Result<()> {
        let command = confined_command(&ConfinementProfile::default(), "true", [] as [&str; 0])?;
        assert_eq!(command.get_program(), std::env::current_exe()?.as_os_str());
        Ok(())
    }
}
