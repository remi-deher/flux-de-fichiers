use super::FlowConnection;

/// Sélectionne la connexion sortante d'un nœud.
///
/// Un port attendu accepte la connexion du même port, ou une connexion sans port.
/// Sans filtre, seules les connexions sans port sont suivies.
pub fn next_connection<'a>(
    connections: &'a [FlowConnection],
    from: &str,
    port_filter: Option<&str>,
) -> Option<&'a FlowConnection> {
    connections.iter().find(|connection| {
        if connection.from != from {
            return false;
        }
        match (port_filter, connection.port.as_deref()) {
            (Some(expected), Some(actual)) => expected == actual,
            (None, None) => true,
            (Some(_), None) => true,
            (None, Some(_)) => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: &str, to: &str, port: Option<&str>) -> FlowConnection {
        FlowConnection {
            from: from.into(),
            to: to.into(),
            port: port.map(str::to_owned),
        }
    }

    #[test]
    fn unconditional_step_ignores_ported_edges() {
        let connections = vec![
            edge("a", "true-branch", Some("true")),
            edge("a", "next", None),
        ];
        assert_eq!(
            next_connection(&connections, "a", None).map(|edge| edge.to.as_str()),
            Some("next")
        );
    }

    #[test]
    fn branch_prefers_the_matching_port_then_an_unported_edge() {
        let matching = vec![
            edge("a", "other", Some("false")),
            edge("a", "yes", Some("true")),
        ];
        assert_eq!(
            next_connection(&matching, "a", Some("true")).map(|edge| edge.to.as_str()),
            Some("yes")
        );

        let fallback = vec![edge("a", "plain", None)];
        assert_eq!(
            next_connection(&fallback, "a", Some("false")).map(|edge| edge.to.as_str()),
            Some("plain")
        );
    }

    #[test]
    fn missing_origin_returns_none() {
        let connections = vec![edge("a", "b", None)];
        assert!(next_connection(&connections, "z", None).is_none());
    }
}
