//! Tiroir d'aperçu pour les éléments déclaratifs du catalogue (Flux, Scripts, Préréglages, Ressources).

use super::flow_preview::FlowDiagram;
use super::metadata::{CommonInfoRows, MetaRow, StatusBanner, StoreTone};
use super::script_preview::{ScriptPreview, catalog_script_path};
use super::uninstall::InstalledActions;
use super::{card_description, section_badge_label};
use crate::route::{Route, use_router};
use crate::ui::{Tag, TagTone};
use forgearr_shared::plugins::CatalogEntry;
use forgearr_shared::plugins::vocab::{CatalogItemType, CatalogSection};
use leptos::prelude::*;

fn usage_hint(entry: &CatalogEntry) -> &'static str {
    match entry.entry_type.section() {
        Some(CatalogSection::Flows) => {
            "Disponible comme modèle prêt à l'emploi lors de la création d'un flux dans l'éditeur."
        }
        Some(CatalogSection::Scripts) => "Exécutable depuis un bloc Script dans l'éditeur de flux.",
        _ => "Ressource synchronisée et utilisable par vos flux et bibliothèques.",
    }
}

#[component]
pub fn CatalogItemDetail(entry: Memo<CatalogEntry>, can_manage: bool) -> impl IntoView {
    let opened = entry.get_untracked();
    // Hors des fiches relues : l'aperçu ne se recharge pas quand une autre donnée de la pièce change.
    let script_preview = (opened.entry_type == CatalogItemType::Script).then(|| {
        view! {
            <ScriptPreview
                path=catalog_script_path(&opened.id)
            />
        }
    });
    view! {
        <div class="inspection-panel">
            {move || view! { <ItemSummary entry=entry.get() /> }}
            {script_preview}
            {move || view! { <ItemInfo entry=entry.get() /> }}
            <InstalledActions item_id=opened.id.clone() can_manage=can_manage />
        </div>
    }
}

/// Bandeau, action et description d'une pièce.
#[component]
fn ItemSummary(entry: CatalogEntry) -> impl IntoView {
    let router = use_router();
    let badge = section_badge_label(entry.entry_type);
    let description = card_description(&entry);
    let hint = usage_hint(&entry);
    let opens_flows = matches!(
        entry.entry_type.section(),
        Some(CatalogSection::Flows | CatalogSection::Scripts)
    );
    let flow_action = opens_flows.then(|| {
        let is_flow = entry.entry_type.section() == Some(CatalogSection::Flows);
        let dest = if is_flow {
            Route::NewFlow
        } else {
            Route::Flows
        };
        let label = if is_flow {
            "Créer un flux"
        } else {
            "Ouvrir les flux"
        };
        view! {
            <button
                type="button"
                class="btn btn-primary btn-sm"
                on:click=move |_| router.go(dest.clone())
            >
                {label}
            </button>
        }
        .into_any()
    });

    view! {
        <StatusBanner
            title=format!("{badge} disponible")
            text=Some(hint.to_owned())
            tone=StoreTone::Neutral
            action=flow_action
        />
        <p class="text-sm">{description}</p>
    }
}

/// Aperçu du flux et métadonnées d'une pièce.
#[component]
fn ItemInfo(entry: CatalogEntry) -> impl IntoView {
    let badge = section_badge_label(entry.entry_type);
    let flow_preview = entry
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.flow_preview.clone());

    view! {
        {flow_preview.map(|preview| view! {
            <FlowDiagram preview=preview />
        })}
        <div class="detail-grid">
            <CommonInfoRows
                id=entry.id
                version=entry.version
                updated_at=entry.updated_at
                author=None
                license=None
                app_min=None
                platforms=Vec::new()
                tags=entry.tags
            />
            <MetaRow term="Type">
                <Tag label=badge tone=TagTone::Primary />
            </MetaRow>
            {entry.capability.map(|cap| view! {
                <MetaRow term="Capacité">
                    <Tag label=cap.as_str().to_owned() tone=TagTone::Success />
                </MetaRow>
            })}
            {entry.category.filter(|cat| cat.shows_badge()).map(|cat| view! {
                <MetaRow term="Catégorie">
                    <Tag label=cat.as_str() tone=TagTone::Neutral />
                </MetaRow>
            })}
            {entry.distribution.map(|dist| view! {
                <MetaRow term="Distribution" mono=true>{dist}</MetaRow>
            })}
            <MetaRow term="Manifeste" mono=true>{entry.manifest_path}</MetaRow>
        </div>
    }
}
