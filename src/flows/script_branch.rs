//! Choix de la sortie d'un bloc Script selon son mode et la réponse du script.

use super::script_runner::ScriptOutcomeStatus;
use forgearr_shared::flows::{BranchPort, ScriptRouting};

/// Sortie suivie en mode conditionnel quand un script réussi ne nomme aucun port.
const DEFAULT_PORT_ON_OK: BranchPort = BranchPort::True;
/// Sortie suivie en mode conditionnel quand un script ignoré ne nomme aucun port.
const DEFAULT_PORT_ON_SKIP: BranchPort = BranchPort::False;

/// Résout le port à suivre. `status` vaut `Ok` ou `Skip` (une erreur arrête le flux avant).
///
/// En mode simple, le port demandé est renvoyé tel quel (port libre ou liaison simple). En mode
/// conditionnel, seul `true`/`false` est accepté et l'absence de port suit le statut.
pub(crate) fn resolve_script_port(
    routing: ScriptRouting,
    status: ScriptOutcomeStatus,
    requested: Option<String>,
) -> Result<Option<String>, String> {
    if routing == ScriptRouting::Single {
        return Ok(requested);
    }
    let port = match requested {
        Some(raw) => BranchPort::parse(&raw).ok_or_else(|| {
            format!(
                "nextPort « {raw} » invalide : attendu « {} » ou « {} »",
                BranchPort::True.as_str(),
                BranchPort::False.as_str()
            )
        })?,
        None if status == ScriptOutcomeStatus::Skip => DEFAULT_PORT_ON_SKIP,
        None => DEFAULT_PORT_ON_OK,
    };
    Ok(Some(port.as_str().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONDITIONAL: ScriptRouting = ScriptRouting::Conditional;

    #[test]
    fn single_keeps_requested_port_and_none() {
        let single = ScriptRouting::Single;
        assert_eq!(
            resolve_script_port(single, ScriptOutcomeStatus::Ok, Some("alt".into())),
            Ok(Some("alt".into()))
        );
        assert_eq!(
            resolve_script_port(single, ScriptOutcomeStatus::Ok, None),
            Ok(None)
        );
        assert_eq!(
            resolve_script_port(single, ScriptOutcomeStatus::Skip, None),
            Ok(None)
        );
    }

    #[test]
    fn conditional_defaults_ok_to_true() {
        assert_eq!(
            resolve_script_port(CONDITIONAL, ScriptOutcomeStatus::Ok, None),
            Ok(Some("true".into()))
        );
    }

    #[test]
    fn conditional_defaults_skip_to_false() {
        assert_eq!(
            resolve_script_port(CONDITIONAL, ScriptOutcomeStatus::Skip, None),
            Ok(Some("false".into()))
        );
    }

    #[test]
    fn conditional_accepts_explicit_false_on_ok() {
        assert_eq!(
            resolve_script_port(CONDITIONAL, ScriptOutcomeStatus::Ok, Some("false".into())),
            Ok(Some("false".into()))
        );
    }

    #[test]
    fn conditional_honours_explicit_true_on_skip() {
        assert_eq!(
            resolve_script_port(CONDITIONAL, ScriptOutcomeStatus::Skip, Some("true".into())),
            Ok(Some("true".into()))
        );
    }

    #[test]
    fn conditional_rejects_unknown_port() {
        let err = resolve_script_port(CONDITIONAL, ScriptOutcomeStatus::Ok, Some("alt".into()))
            .unwrap_err();
        assert!(err.contains("alt"));
    }
}
