//! Domaine Réglages : vue d'ensemble du central (tuiles et points à surveiller).

use crate::shared::ui::{Card, DetailRow, IconName, MetricCard, SectionHeader};
use leptos::prelude::*;

/// Valeur affichée tant que la source de la tuile n'a pas répondu.
const PENDING_VALUE: &str = "—";

/// Une tuile : une métrique cliquable qui ouvre la section où elle se gère.
struct Tile {
    id: &'static str,
    label: &'static str,
    subtitle: &'static str,
    icon: IconName,
    /// Onglet de la sous-barre à ouvrir au clic.
    opens: &'static str,
}

const TILES: [Tile; 4] = [
    Tile {
        id: "settings-tile-workers",
        label: "Workers en ligne",
        subtitle: "Central et workers",
        icon: IconName::Workers,
        opens: super::TAB_STORAGE,
    },
    Tile {
        id: "settings-tile-accounts",
        label: "Comptes actifs",
        subtitle: "Utilisateurs autorisés",
        icon: IconName::Lock,
        opens: super::TAB_USERS,
    },
    Tile {
        id: "settings-tile-keys",
        label: "Clés d'API actives",
        subtitle: "Machines raccordées",
        icon: IconName::Shield,
        opens: super::TAB_KEYS,
    },
    Tile {
        id: "settings-tile-database",
        label: "Base de données",
        subtitle: "SQLite, journal compris",
        icon: IconName::Storage,
        opens: super::TAB_STORAGE,
    },
];

fn tile_view(tile: &Tile) -> impl IntoView + use<> {
    view! {
        <button type="button" class="settings-tile" data-subtab-go=tile.opens aria-label=tile.label>
            <MetricCard
                label=tile.label
                value=PENDING_VALUE
                id=tile.id
                subtitle=tile.subtitle
                icon=tile.icon
            />
        </button>
    }
}

#[component]
pub fn SettingsOverview() -> impl IntoView {
    view! {
        <section class="metrics-grid" aria-label="Chiffres clés du central">
            {TILES.iter().map(tile_view).collect_view()}
        </section>
        <Card aria_labelledby="settings-alerts-title">
            <SectionHeader
                id="settings-alerts-title"
                title="À surveiller"
                subtitle="Ce qui mérite votre attention. Chaque ligne ouvre la section concernée."
            />
            <div id="settings-alerts" class="settings-alerts" aria-live="polite">
                <p class="form-help">"Chargement…"</p>
            </div>
            <div class="detail-grid">
                <DetailRow label="Version" id="settings-overview-version" />
            </div>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <SettingsOverview /> }.to_html()
    }

    #[test]
    fn test_four_tiles_each_open_a_section() {
        let html = render();
        assert_eq!(html.matches("class=\"settings-tile\"").count(), TILES.len());
        for tile in &TILES {
            assert!(html.contains(&format!("id=\"{}\"", tile.id)), "{}", tile.id);
            assert!(html.contains(&format!("data-subtab-go=\"{}\"", tile.opens)));
        }
    }

    #[test]
    fn test_tile_ids_are_unique() {
        let mut ids: Vec<&str> = TILES.iter().map(|tile| tile.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TILES.len());
    }

    #[test]
    fn test_alerts_and_version_have_the_ids_used_by_the_client() {
        let html = render();
        for id in ["settings-alerts", "settings-overview-version"] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(!html.contains("style="));
    }
}
