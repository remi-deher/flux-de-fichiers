//! Tiroirs d'inspection des Extensions, liés à l'adresse (ADR 0035) : `?voir=extension.<id>`,
//! `pack.<id>`, `outil.<id>` ou `element.<id>`. Un seul tiroir est ouvert à la fois.

use super::SelectedItem;
use crate::location::DETAIL_PARAM;
use crate::url_state::{DrawerKind, detail_value, parse_detail, use_param};
use forgearr_shared::plugins::{CatalogEntry, ToolStatusSummary};
use leptos::prelude::*;

#[derive(Clone, PartialEq)]
pub(super) enum InspectionTarget {
    Plugin(SelectedItem),
    Pack(SelectedItem),
    Tool(SelectedItem),
    Item(Box<CatalogEntry>),
}

impl InspectionTarget {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Plugin(s) | Self::Pack(s) | Self::Tool(s) => &s.id,
            Self::Item(entry) => &entry.id,
        }
    }

    pub(super) fn title(&self) -> String {
        match self {
            Self::Plugin(s) | Self::Pack(s) | Self::Tool(s) => s.name.clone(),
            Self::Item(entry) => entry.name.clone(),
        }
    }

    pub(super) fn kind(&self) -> DrawerKind {
        match self {
            Self::Plugin(_) => DrawerKind::Extension,
            Self::Pack(_) => DrawerKind::Pack,
            Self::Tool(_) => DrawerKind::Tool,
            Self::Item(_) => DrawerKind::CatalogItem,
        }
    }

    /// Cible désignée par `?voir=<kind>.<id>`. Une pièce du catalogue absente de `entries` reste
    /// introuvable (`None`) ; les autres s'ouvrent sous leur identifiant si leur nom n'est pas connu.
    fn resolve(
        kind: DrawerKind,
        id: &str,
        entries: &[CatalogEntry],
        tools: &[ToolStatusSummary],
    ) -> Option<Self> {
        let entry = entries.iter().find(|entry| entry.id == id);
        let item = |name: Option<&str>| SelectedItem {
            id: id.to_owned(),
            name: name.unwrap_or(id).to_owned(),
        };
        match kind {
            DrawerKind::Extension => Some(Self::Plugin(item(entry.map(|e| e.name.as_str())))),
            DrawerKind::Pack => Some(Self::Pack(item(entry.map(|e| e.name.as_str())))),
            DrawerKind::Tool => {
                let tool = tools.iter().find(|tool| tool.id == id);
                Some(Self::Tool(item(tool.map(|t| t.name.as_str()))))
            }
            DrawerKind::CatalogItem => entry.map(|e| Self::Item(Box::new(e.clone()))),
            _ => None,
        }
    }
}

/// Cible et ouverture des quatre tiroirs d'inspection.
#[derive(Clone, Copy)]
pub(super) struct InspectionDrawers {
    pub target: RwSignal<Option<InspectionTarget>>,
    pub plugin: RwSignal<bool>,
    pub pack: RwSignal<bool>,
    pub tool: RwSignal<bool>,
    pub item: RwSignal<bool>,
}

impl InspectionDrawers {
    pub(super) fn new() -> Self {
        Self {
            target: RwSignal::new(None),
            plugin: RwSignal::new(false),
            pack: RwSignal::new(false),
            tool: RwSignal::new(false),
            item: RwSignal::new(false),
        }
    }

    /// Ouverture du tiroir de ce type, `None` pour un type qui n'est pas un tiroir d'inspection.
    fn open_signal(self, kind: DrawerKind) -> Option<RwSignal<bool>> {
        match kind {
            DrawerKind::Extension => Some(self.plugin),
            DrawerKind::Pack => Some(self.pack),
            DrawerKind::Tool => Some(self.tool),
            DrawerKind::CatalogItem => Some(self.item),
            _ => None,
        }
    }

    fn all(self) -> [RwSignal<bool>; 4] {
        [self.plugin, self.pack, self.tool, self.item]
    }

    /// Ouvre `target` dans son tiroir et ferme les autres.
    pub(super) fn show(self, target: InspectionTarget) {
        let opening = self.open_signal(target.kind());
        for open in self.all() {
            if Some(open) != opening {
                open.set(false);
            }
        }
        self.target.set(Some(target));
        if let Some(open) = opening {
            open.set(true);
        }
    }

    pub(super) fn any_open(self) -> bool {
        self.all().into_iter().any(|open| open.get())
    }

    /// Valeur de `?voir=` qui décrit le tiroir ouvert.
    fn wanted(self) -> Option<String> {
        let target = self.target.get();
        self.any_open()
            .then(|| target.map(|t| detail_value(&t.kind().slug(), t.id())))
            .flatten()
    }

    fn is_ours(self, value: Option<&str>) -> bool {
        value
            .and_then(parse_detail)
            .is_some_and(|(kind, _)| self.open_signal(kind).is_some())
    }

    /// Lie les tiroirs à `?voir=` : l'adresse ouvre le tiroir, et le tiroir écrit l'adresse.
    pub(super) fn sync_with_url(
        self,
        entries: Signal<Vec<CatalogEntry>>,
        tools: Signal<Vec<ToolStatusSummary>>,
    ) {
        let param = use_param(DETAIL_PARAM);
        Effect::new(move |_| {
            let value = param.get();
            let Some((kind, id)) = value.as_deref().and_then(parse_detail) else {
                if value.is_none() && self.any_open_untracked() {
                    self.all().into_iter().for_each(|open| open.set(false));
                }
                return;
            };
            if self.open_signal(kind).is_none() {
                return;
            }
            let shown = self.target.get_untracked();
            let already = shown
                .as_ref()
                .is_some_and(|t| t.kind() == kind && t.id() == id);
            if already && self.open_signal(kind).is_some_and(|open| open.get_untracked()) {
                return;
            }
            // Le catalogue peut arriver après l'adresse : lire ces listes relance l'effet à leur arrivée.
            if let Some(target) = InspectionTarget::resolve(kind, &id, &entries.get(), &tools.get())
            {
                self.show(target);
            }
        });
        Effect::new(move |previous: Option<()>| {
            let wanted = self.wanted();
            if previous.is_none() {
                return;
            }
            let current = param.get_untracked();
            if wanted != current && (wanted.is_some() || self.is_ours(current.as_deref())) {
                param.set(wanted);
            }
        });
    }

    fn any_open_untracked(self) -> bool {
        self.all().into_iter().any(|open| open.get_untracked())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_resolved_from_the_address_by_kind() {
        let plugin = InspectionTarget::resolve(DrawerKind::Extension, "x.y", &[], &[]);
        let title = plugin.as_ref().map(InspectionTarget::title);
        assert_eq!(title.as_deref(), Some("x.y"));
        assert_eq!(plugin.map(|t| t.kind()), Some(DrawerKind::Extension));
        let tool = InspectionTarget::resolve(DrawerKind::Tool, "ffmpeg", &[], &[]);
        assert_eq!(tool.map(|t| t.kind()), Some(DrawerKind::Tool));
    }

    #[test]
    fn a_catalogue_item_missing_from_the_catalogue_stays_unresolved() {
        assert!(InspectionTarget::resolve(DrawerKind::CatalogItem, "absent", &[], &[]).is_none());
        assert!(InspectionTarget::resolve(DrawerKind::Worker, "1", &[], &[]).is_none());
    }
}
