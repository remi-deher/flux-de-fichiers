//! Réseau du central : adresse d'écoute, TLS, origine web et durée de session.

use super::system::{DetailRow, StatusSection, duration_label};
use flux_shared::system::SystemStatus;
use leptos::prelude::*;

fn tls_label(direct: bool) -> &'static str {
    if direct {
        "Servi par le central"
    } else {
        "Non (HTTP simple, proxy TLS externe conseillé)"
    }
}

fn network_rows(status: &SystemStatus) -> Vec<DetailRow> {
    let network = &status.network;
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

#[component]
pub fn NetworkSection() -> impl IntoView {
    let footer = view! {
        <p class="form-help">
            "Ces réglages viennent de l'environnement du central (BIND_ADDR, TLS_CERT, TLS_KEY, WEB_ORIGIN) ; les changer demande de redémarrer le conteneur."
        </p>
    }.into_any();
    view! {
        <StatusSection title="Réseau" subtitle="Port HTTP unique, TLS et sessions web."
            rows=network_rows footer=footer />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_is_described_either_way() {
        assert!(tls_label(true).contains("central"));
        assert!(tls_label(false).contains("HTTP simple"));
    }
}
