//! Mode de sortie du bloc Script et sorties des blocs conditionnels (Condition, Script).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Clé de la configuration d'un bloc Script qui porte son mode de sortie.
pub const SCRIPT_ROUTING_KEY: &str = "routing";

/// Mode de sortie du bloc Script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScriptRouting {
    /// Sortie simple : le script peut nommer un port libre, sinon l'unique liaison est suivie.
    #[default]
    Single,
    /// Branche Vrai / Faux.
    Conditional,
}

impl ScriptRouting {
    /// Les deux modes, dans l'ordre d'affichage.
    pub const ALL: [Self; 2] = [Self::Single, Self::Conditional];

    /// Lit `config.routing` ; une valeur absente ou inconnue vaut `Single` (compatibilité).
    pub fn from_config(config: &Value) -> Self {
        config
            .get(SCRIPT_ROUTING_KEY)
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// Reconnaît `"single"` et `"conditional"` ; toute autre valeur vaut `Single`.
    pub fn parse(raw: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|routing| routing.as_str() == raw)
            .unwrap_or_default()
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Conditional => "conditional",
        }
    }
}

/// Sortie d'un bloc conditionnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BranchPort {
    True,
    False,
}

impl BranchPort {
    /// Les deux sorties, dans l'ordre d'affichage.
    pub const ALL: [Self; 2] = [Self::True, Self::False];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
        }
    }

    pub const fn from_bool(matched: bool) -> Self {
        if matched { Self::True } else { Self::False }
    }

    /// Reconnaît `"true"` et `"false"` uniquement.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|port| port.as_str() == raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routing_reads_config_with_fallback() {
        assert_eq!(
            ScriptRouting::from_config(&json!({})),
            ScriptRouting::Single
        );
        assert_eq!(
            ScriptRouting::from_config(&json!({"routing": "single"})),
            ScriptRouting::Single
        );
        assert_eq!(
            ScriptRouting::from_config(&json!({"routing": "conditional"})),
            ScriptRouting::Conditional
        );
        assert_eq!(
            ScriptRouting::from_config(&json!({"routing": "inconnu"})),
            ScriptRouting::Single
        );
        assert_eq!(
            ScriptRouting::from_config(&json!({"routing": 3})),
            ScriptRouting::Single
        );
        assert_eq!(ScriptRouting::parse("autre"), ScriptRouting::Single);
    }

    #[test]
    fn routing_as_str_round_trips_through_serde() -> serde_json::Result<()> {
        for routing in ScriptRouting::ALL {
            assert_eq!(ScriptRouting::parse(routing.as_str()), routing);
            let encoded = serde_json::to_value(routing)?;
            assert_eq!(encoded, json!(routing.as_str()));
            assert_eq!(serde_json::from_value::<ScriptRouting>(encoded)?, routing);
        }
        Ok(())
    }

    #[test]
    fn branch_port_parses_and_formats() -> serde_json::Result<()> {
        assert_eq!(BranchPort::parse("true"), Some(BranchPort::True));
        assert_eq!(BranchPort::parse("false"), Some(BranchPort::False));
        assert_eq!(BranchPort::parse("alt"), None);
        assert_eq!(BranchPort::parse("True"), None);
        assert_eq!(BranchPort::from_bool(true), BranchPort::True);
        assert_eq!(BranchPort::from_bool(false), BranchPort::False);
        for port in BranchPort::ALL {
            let encoded = serde_json::to_value(port)?;
            assert_eq!(encoded, json!(port.as_str()));
            assert_eq!(serde_json::from_value::<BranchPort>(encoded)?, port);
        }
        Ok(())
    }
}
