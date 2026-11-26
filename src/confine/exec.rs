//! Sous-commande `sandbox-exec` : s'applique un profil puis remplace le processus par le programme
//! demandé (ADR 0034, point 5).
//!
//! Elle s'exécute avant toute autre initialisation de l'exécutable (journaux, runtime asynchrone), dans un
//! processus à un seul fil : les restrictions de Landlock et de seccomp valent pour le fil qui les pose, et
//! `exec` conserve ce fil.

use super::{
    ConfineError,
    command::{ARGUMENTS_SEPARATOR, PROFILE_FLAG},
    landlock, limits, seccomp,
};
use forgearr_shared::confinement::ConfinementProfile;
use std::{os::unix::process::CommandExt, process::Command};

/// Le profil est illisible ou une restriction n'a pas pu être appliquée : le programme n'est pas lancé.
pub const EXIT_SETUP_FAILED: i32 = 125;
/// Les restrictions sont en place mais le programme n'a pas pu être lancé.
pub const EXIT_LAUNCH_FAILED: i32 = 126;

/// Un lancement demandé : profil, programme et arguments.
#[derive(Debug, PartialEq, Eq)]
struct Invocation {
    profile: ConfinementProfile,
    program: String,
    arguments: Vec<String>,
}

/// Analyse `--profile <json> -- <programme> [arguments]`.
fn parse_invocation(arguments: &[String]) -> Result<Invocation, ConfineError> {
    let mut items = arguments.iter();
    if items.next().map(String::as_str) != Some(PROFILE_FLAG) {
        return Err(ConfineError::Usage);
    }
    let Some(profile_json) = items.next() else {
        return Err(ConfineError::Usage);
    };
    if items.next().map(String::as_str) != Some(ARGUMENTS_SEPARATOR) {
        return Err(ConfineError::Usage);
    }
    let Some(program) = items.next() else {
        return Err(ConfineError::Usage);
    };
    Ok(Invocation {
        profile: serde_json::from_str(profile_json)?,
        program: program.clone(),
        arguments: items.cloned().collect(),
    })
}

fn set_no_new_privileges() -> Result<(), ConfineError> {
    let enabled = libc::c_ulong::from(1_u8);
    let unused = libc::c_ulong::from(0_u8);
    // SAFETY: `PR_SET_NO_NEW_PRIVS` ne lit ni n'écrit aucune mémoire.
    let result = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, enabled, unused, unused, unused) };
    if result != 0 {
        return Err(super::last_os_error("PR_SET_NO_NEW_PRIVS"));
    }
    Ok(())
}

/// Pose, dans l'ordre : plafonds, interdiction de gagner des privilèges, Landlock, puis seccomp (qui
/// refuse des appels que les étapes précédentes utilisent).
fn apply_confinement(profile: &ConfinementProfile) -> Result<(), ConfineError> {
    limits::apply(&profile.limits)?;
    set_no_new_privileges()?;
    landlock::restrict(profile, landlock::abi())?;
    seccomp::install(profile.enforcement)
}

/// Point d'entrée de `forgearr sandbox-exec <arguments>` ; ne rend la main que si le lancement a échoué,
/// avec le code de sortie du processus.
pub fn run_sandbox_exec(arguments: &[String]) -> i32 {
    let invocation = match parse_invocation(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("sandbox-exec : {error}");
            return EXIT_SETUP_FAILED;
        }
    };
    if let Err(error) = apply_confinement(&invocation.profile) {
        eprintln!("sandbox-exec : {error}");
        return EXIT_SETUP_FAILED;
    }
    let source = Command::new(&invocation.program)
        .args(&invocation.arguments)
        .exec();
    let error = ConfineError::Launch {
        program: invocation.program,
        source,
    };
    eprintln!("sandbox-exec : {error}");
    EXIT_LAUNCH_FAILED
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::confinement::NetworkAccess;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn a_complete_invocation_is_parsed() -> Result<(), ConfineError> {
        let json = r#"{"network":"any"}"#;
        let arguments = strings(&[
            PROFILE_FLAG,
            json,
            ARGUMENTS_SEPARATOR,
            "ty",
            "server",
            "-v",
        ]);
        let invocation = parse_invocation(&arguments)?;
        assert_eq!(invocation.profile.network, NetworkAccess::Any);
        assert_eq!(invocation.program, "ty");
        assert_eq!(invocation.arguments, strings(&["server", "-v"]));
        Ok(())
    }

    #[test]
    fn a_program_without_arguments_is_accepted() -> Result<(), ConfineError> {
        let arguments = strings(&[PROFILE_FLAG, "{}", ARGUMENTS_SEPARATOR, "true"]);
        assert!(parse_invocation(&arguments)?.arguments.is_empty());
        Ok(())
    }

    #[test]
    fn an_incomplete_invocation_is_a_usage_error() {
        let incomplete = [
            strings(&[]),
            strings(&["--other", "{}", "--", "true"]),
            strings(&[PROFILE_FLAG]),
            strings(&[PROFILE_FLAG, "{}"]),
            strings(&[PROFILE_FLAG, "{}", "true"]),
            strings(&[PROFILE_FLAG, "{}", ARGUMENTS_SEPARATOR]),
        ];
        for arguments in incomplete {
            assert!(matches!(
                parse_invocation(&arguments),
                Err(ConfineError::Usage)
            ));
        }
    }

    #[test]
    fn an_unreadable_profile_is_reported() {
        let arguments = strings(&[PROFILE_FLAG, "pas du json", ARGUMENTS_SEPARATOR, "true"]);
        assert!(matches!(
            parse_invocation(&arguments),
            Err(ConfineError::Profile(_))
        ));
    }

    #[test]
    fn a_bad_invocation_exits_with_the_setup_code() {
        assert_eq!(run_sandbox_exec(&[]), EXIT_SETUP_FAILED);
    }

    #[test]
    fn the_two_failure_codes_differ_from_each_other() {
        assert_ne!(EXIT_SETUP_FAILED, EXIT_LAUNCH_FAILED);
    }
}
