//! Onglet Réseau : configuration du central, connectivité des workers et diagnostic.

use super::network_diagnostics::DiagnosticsCard;
use super::network_workers::WorkersConnectivityCard;
use super::system::{DetailGrid, DetailRow, duration_label};
use crate::api;
use crate::ui::SectionHeader;
use flux_shared::system::{NetworkStatus, SystemStatus};
use leptos::prelude::*;

const STATUS_PATH: &str = "/api/v1/system/status";

fn tls_label(direct: bool) -> &'static str {
    if direct {
        "Servi par le central"
    } else {
        "Non (HTTP simple, proxy TLS externe conseillé)"
    }
}

fn network_rows(network: &NetworkStatus) -> Vec<DetailRow> {
    vec![
        DetailRow::mono("Adresse d'écoute", network.bind.clone()),
        DetailRow::text("TLS", tls_label(network.tls)),
        DetailRow::mono(
            "Origine web",
            network
                .web_origin
                .clone()
                .unwrap_or_else(|| "Celle de l'adresse d'accès".to_owned()),
        ),
        DetailRow::text(
            "Durée d'une session",
            duration_label(network.session_ttl_seconds),
        ),
    ]
}

/// Bloc d'environnement du central, à copier dans `compose.yaml` ou un fichier `.env` ; sans secret.
fn environment_block(network: &NetworkStatus) -> String {
    let origin = network.web_origin.as_deref().map_or_else(
        || "# WEB_ORIGIN non défini".to_owned(),
        |origin| format!("WEB_ORIGIN={origin}"),
    );
    let tls = if network.tls {
        "TLS_CERT=/chemin/certificat.pem\nTLS_KEY=/chemin/cle.pem"
    } else {
        "# TLS_CERT / TLS_KEY non définis (TLS par un proxy externe)"
    };
    format!(
        "BIND_ADDR={}\n{origin}\n{tls}\nWEB_SESSION_TTL_SECONDS={}\n",
        network.bind, network.session_ttl_seconds
    )
}

#[component]
fn ConfigurationCard() -> impl IntoView {
    let status = LocalResource::new(|| api::get::<SystemStatus>(STATUS_PATH));
    view! {
        <div class="settings-section">
            <SectionHeader title="Configuration" subtitle="Port HTTP unique, TLS et sessions web (lecture seule)." />
            {move || match status.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(status)) => view! {
                    <DetailGrid rows=network_rows(&status.network) />
                    <pre class="code-block-wrapper"><code>{environment_block(&status.network)}</code></pre>
                }.into_any(),
            }}
            <p class="form-help">
                "Ces réglages viennent de l'environnement du central ; les changer demande de redémarrer le conteneur."
            </p>
        </div>
    }
}

#[component]
pub fn NetworkSection() -> impl IntoView {
    view! {
        <ConfigurationCard />
        <WorkersConnectivityCard />
        <DiagnosticsCard />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(tls: bool, web_origin: Option<&str>) -> NetworkStatus {
        NetworkStatus {
            bind: "0.0.0.0:3000".into(),
            tls,
            web_origin: web_origin.map(str::to_owned),
            session_ttl_seconds: 28_800,
        }
    }

    #[test]
    fn tls_is_described_either_way() {
        assert!(tls_label(true).contains("central"));
        assert!(tls_label(false).contains("HTTP simple"));
    }

    #[test]
    fn rows_cover_the_four_settings() {
        assert_eq!(network_rows(&network(false, None)).len(), 4);
    }

    #[test]
    fn environment_block_mirrors_the_configuration_without_secrets() {
        let block = environment_block(&network(true, Some("https://flux.example")));
        assert!(block.contains("BIND_ADDR=0.0.0.0:3000"));
        assert!(block.contains("WEB_ORIGIN=https://flux.example"));
        assert!(block.contains("TLS_CERT="));
        assert!(block.contains("WEB_SESSION_TTL_SECONDS=28800"));
        let plain = environment_block(&network(false, None));
        assert!(plain.contains("# WEB_ORIGIN non défini"));
        assert!(!plain.contains("TLS_CERT=/"));
    }
}
