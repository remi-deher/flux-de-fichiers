//! Politique réseau partagée pour les appels sortants (dépôts de plugins, webhooks, *arr).
//!
//! Mutualise le contrôle de schéma, la protection SSRF (blocage des métadonnées cloud, adresses
//! link-local, multicast, non spécifiées, et adresses privées selon le contexte) ainsi que
//! l'épinglage DNS multi-adresses pour éviter le DNS rebinding.

use reqwest::{Client, Url, redirect::Policy};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use thiserror::Error;

/// Variable d'environnement autorisant les adresses privées pour les dépôts de plugins (labo/tests).
pub const ALLOW_PRIVATE_HOSTS_ENV: &str = "PLUGIN_ALLOW_PRIVATE_HOSTS";
/// Variable d'environnement bloquant aussi les IP privées/loopback sur webhooks et *arr.
pub const OUTBOUND_BLOCK_PRIVATE_ENV: &str = "OUTBOUND_BLOCK_PRIVATE_HOSTS";

const UNIQUE_LOCAL_PREFIX_MASK: u16 = 0xfe00;
const UNIQUE_LOCAL_PREFIX: u16 = 0xfc00;

#[derive(Debug, Error)]
pub enum NetPolicyError {
    #[error("URL invalide : {0}")]
    InvalidUrl(String),
    #[error("hôte refusé : {0}")]
    ForbiddenHost(String),
    #[error("résolution réseau impossible : {0}")]
    Network(String),
}

/// Règles applicables à une requête sortante.
#[derive(Clone, Copy, Debug, Default)]
pub struct OutboundPolicy {
    pub allow_private_hosts: bool,
    pub require_https: bool,
}

impl OutboundPolicy {
    /// Politique des dépôts de plugins : IP publiques uniquement par défaut.
    pub fn plugin_repo_from_env() -> Self {
        Self {
            allow_private_hosts: std::env::var(ALLOW_PRIVATE_HOSTS_ENV).is_ok_and(|v| v == "1"),
            require_https: false,
        }
    }

    /// Politique des intégrations/webhooks : bloque toujours link-local/métadonnées, et les IP
    /// privées seulement si `OUTBOUND_BLOCK_PRIVATE_HOSTS=1`.
    pub fn integration_from_env() -> Self {
        Self {
            allow_private_hosts: !std::env::var(OUTBOUND_BLOCK_PRIVATE_ENV).is_ok_and(|v| v == "1"),
            require_https: false,
        }
    }
}

/// Adresses toujours illégitimes pour un service applicatif (métadonnées cloud, link-local, multicast).
pub fn is_always_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_always_forbidden_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(mapped) => is_always_forbidden_v4(mapped),
            None => is_always_forbidden_v6(v6),
        },
    }
}

/// Toute adresse interne/non routable publiquement (pour les dépôts distants non approuvés).
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_always_forbidden_v4(v4) || v4.is_loopback() || v4.is_private(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(mapped) => {
                is_always_forbidden_v4(mapped) || mapped.is_loopback() || mapped.is_private()
            }
            None => {
                let first_segment = v6.segments()[0];
                is_always_forbidden_v6(v6)
                    || v6.is_loopback()
                    || first_segment & UNIQUE_LOCAL_PREFIX_MASK == UNIQUE_LOCAL_PREFIX
            }
        },
    }
}

fn is_always_forbidden_v4(ip: Ipv4Addr) -> bool {
    ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || ip.is_multicast()
}

fn is_always_forbidden_v6(ip: Ipv6Addr) -> bool {
    ip.is_unspecified() || ip.is_multicast() || ip.is_unicast_link_local()
}

/// Valide l'URL, résout son hôte et vérifie toutes les adresses retournées selon `policy`.
pub async fn resolve_checked(
    url: &Url,
    policy: OutboundPolicy,
) -> Result<(String, Vec<SocketAddr>), NetPolicyError> {
    if policy.require_https {
        if url.scheme() != "https" {
            return Err(NetPolicyError::InvalidUrl("schéma https requis".into()));
        }
    } else if !matches!(url.scheme(), "http" | "https") {
        return Err(NetPolicyError::InvalidUrl(
            "schéma http ou https attendu".into(),
        ));
    }

    let port = url
        .port_or_known_default()
        .ok_or_else(|| NetPolicyError::InvalidUrl("port inconnu".into()))?;

    let (host_label, addresses): (String, Vec<SocketAddr>) = match url.domain() {
        Some(domain) => {
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((domain, port))
                .await
                .map_err(|error| NetPolicyError::Network(error.to_string()))?
                .collect();
            (domain.to_owned(), resolved)
        }
        None => {
            let literal = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
            let ip: IpAddr = literal
                .parse()
                .map_err(|_| NetPolicyError::InvalidUrl("hôte invalide".into()))?;
            (literal.to_owned(), vec![SocketAddr::new(ip, port)])
        }
    };

    if addresses.is_empty() {
        return Err(NetPolicyError::Network(format!(
            "{host_label} : aucune adresse"
        )));
    }

    let blocked = addresses.iter().any(|address| {
        if policy.allow_private_hosts {
            is_always_forbidden_ip(address.ip())
        } else {
            is_forbidden_ip(address.ip())
        }
    });
    if blocked {
        return Err(NetPolicyError::ForbiddenHost(host_label));
    }

    Ok((host_label, addresses))
}

/// Construit un client HTTP sans redirection automatique et épinglé sur toutes les IP validées.
pub fn build_pinned_client(
    url: &Url,
    addresses: &[SocketAddr],
    timeout: Duration,
) -> Result<Client, NetPolicyError> {
    let mut builder = Client::builder().redirect(Policy::none()).timeout(timeout);
    if let Some(domain) = url.domain() {
        builder = builder.resolve_to_addrs(domain, addresses);
    }
    builder
        .build()
        .map_err(|error| NetPolicyError::Network(error.to_string()))
}

/// Valide une URL de webhook ou d'intégration avant envoi (sans figer le client partagé).
pub async fn validate_outbound_url(
    raw: &str,
    policy: OutboundPolicy,
) -> Result<Url, NetPolicyError> {
    let url =
        Url::parse(raw.trim()).map_err(|error| NetPolicyError::InvalidUrl(error.to_string()))?;
    let _ = resolve_checked(&url, policy).await?;
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_ip_blocks_private_loopback_and_metadata() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.50",
            "169.254.169.254",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:192.168.0.1",
        ] {
            let parsed: IpAddr = ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert!(is_forbidden_ip(parsed), "{ip}");
        }
        for ip in ["8.8.8.8", "140.82.121.4", "2606:4700::1111"] {
            let parsed: IpAddr = ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert!(!is_forbidden_ip(parsed), "{ip}");
        }
    }

    #[test]
    fn always_forbidden_blocks_metadata_even_when_private_lan_is_allowed() {
        for ip in ["169.254.169.254", "0.0.0.0", "224.0.0.1", "fe80::1"] {
            let parsed: IpAddr = ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert!(is_always_forbidden_ip(parsed), "{ip}");
        }
        for ip in ["127.0.0.1", "192.168.1.10", "10.0.0.5"] {
            let parsed: IpAddr = ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
            assert!(!is_always_forbidden_ip(parsed), "{ip}");
        }
    }

    #[tokio::test]
    async fn validate_outbound_url_rejects_bad_schemes_and_metadata() {
        let policy = OutboundPolicy {
            allow_private_hosts: true,
            require_https: false,
        };
        assert!(
            validate_outbound_url("file:///etc/passwd", policy)
                .await
                .is_err()
        );
        assert!(
            validate_outbound_url("http://169.254.169.254/latest/meta-data", policy)
                .await
                .is_err()
        );
        assert!(
            validate_outbound_url("http://127.0.0.1:8080/hook", policy)
                .await
                .is_ok()
        );

        let strict = OutboundPolicy {
            allow_private_hosts: false,
            require_https: false,
        };
        assert!(
            validate_outbound_url("http://127.0.0.1:8080/hook", strict)
                .await
                .is_err()
        );
    }
}
