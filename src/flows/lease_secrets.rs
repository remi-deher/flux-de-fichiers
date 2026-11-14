//! Secrets qui voyagent avec le bail vers un worker distant : seulement ceux que le flux figé utilise.
//!
//! Le worker distant n'a pas la clé de chiffrement du central : il reçoit donc le clair, mais
//! uniquement pour les connexions *arr* des services dont le flux contient un bloc, et jamais
//! pour une tâche sans flux. Le transport reste protégé par le TLS du reverse proxy.

use super::{ArrContext, ArrLeaseContext, ArrService, FlowBlockKind, FlowGraph};
use std::collections::HashSet;

fn service_of(kind: &FlowBlockKind) -> Option<ArrService> {
    match kind {
        FlowBlockKind::SonarrRescan | FlowBlockKind::SonarrRename => Some(ArrService::Sonarr),
        FlowBlockKind::RadarrRescan | FlowBlockKind::RadarrRename => Some(ArrService::Radarr),
        _ => None,
    }
}

/// Services *arr dont la définition contient au moins un bloc ; vide si elle est illisible.
pub fn arr_services_in(definition: &str) -> HashSet<ArrService> {
    serde_json::from_str::<FlowGraph>(definition)
        .map(|graph| {
            graph
                .nodes
                .iter()
                .filter_map(|node| service_of(&node.kind))
                .collect()
        })
        .unwrap_or_default()
}

/// Contexte *arr limité aux services utilisés ; `None` s'il n'en reste aucun.
pub fn lease_arr_context(definition: &str, arr: &ArrContext) -> Option<ArrLeaseContext> {
    let used = arr_services_in(definition);
    let connections: std::collections::HashMap<_, _> = arr
        .connections
        .iter()
        .filter(|(service, _)| used.contains(service))
        .map(|(service, connection)| (*service, connection.clone()))
        .collect();
    (!connections.is_empty()).then(|| ArrLeaseContext {
        connections,
        media_root: arr.media_root.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::ArrConnection;
    use serde_json::json;

    fn context() -> ArrContext {
        let connection = |url: &str| ArrConnection {
            url: url.into(),
            api_key: "secret".into(),
            path_local: String::new(),
            path_remote: String::new(),
        };
        ArrContext {
            connections: [
                (ArrService::Sonarr, connection("http://sonarr")),
                (ArrService::Radarr, connection("http://radarr")),
            ]
            .into(),
            media_root: "/media".into(),
        }
    }

    fn definition(kinds: &[&str]) -> String {
        let nodes: Vec<_> = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| json!({"id": format!("n{index}"), "type": kind, "name": "x"}))
            .collect();
        json!({"nodes": nodes, "connections": []}).to_string()
    }

    #[test]
    fn services_come_from_the_blocks_of_the_flow() {
        let used = arr_services_in(&definition(&["sonarr-rescan", "file-output"]));
        assert_eq!(used, HashSet::from([ArrService::Sonarr]));
        assert!(arr_services_in("pas du json").is_empty());
    }

    #[test]
    fn lease_carries_only_the_connections_the_flow_uses() -> Result<(), &'static str> {
        let lease = lease_arr_context(&definition(&["radarr-rename"]), &context())
            .ok_or("contexte attendu")?;
        assert_eq!(lease.connections.len(), 1);
        assert!(lease.connections.contains_key(&ArrService::Radarr));
        assert_eq!(lease.media_root, std::path::PathBuf::from("/media"));
        Ok(())
    }

    #[test]
    fn lease_has_no_secret_without_an_arr_block() {
        assert!(lease_arr_context(&definition(&["file-output"]), &context()).is_none());
    }
}
