//! Parcours des cibles et de ce qu'elles exigent : quoi installer, mettre à jour ou réparer, et depuis où.

use super::{
    after::engine_after,
    context::{PlanContext, Standing},
    declared::Declared,
    findings::{Findings, message_of},
    needs::{Need, Requester, Strictness, needs_of, needs_of_graph},
    states::available_update,
};
use crate::{
    error::{AppError, AppResult},
    plugins::{
        CatalogItemType,
        installed::{
            EngineCatalog, ItemContent, script_copy::files_of_copy, usage::elements_declared_by,
        },
        read_pack_installation,
    },
    repository::InstallationRecord,
};
use forgearr_shared::{
    plugins::store::{
        ItemOrigin, ItemRef, LOCAL_SOURCE_ID,
        plan::{PlanAction, PlanBlocker, PlanReason, PlanWarning},
    },
    version::satisfies,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
};

/// Contenu à figer.
pub(super) struct Change {
    pub source: usize,
    pub action: PlanAction,
    pub content: ItemContent,
    pub declared: Declared,
    pub previous: Option<Previous>,
}

/// Ce que l'élément était avant.
pub(super) struct Previous {
    pub version: String,
    /// `None` : sa copie figée n'est plus lisible.
    pub copy: Option<(ItemContent, Declared)>,
}

/// Élément du plan.
pub(super) struct Selected {
    pub id: String,
    pub name: String,
    pub kind: CatalogItemType,
    pub version: String,
    /// Version qu'une exigence compare.
    pub effective_version: String,
    pub origin: ItemOrigin,
    pub reason: PlanReason,
    pub required_by: Vec<String>,
    /// `None` : déjà installé.
    pub change: Option<Change>,
}

/// Place d'un élément dans le plan.
struct Place {
    reason: PlanReason,
    required_by: Option<String>,
}

impl From<&Need> for Place {
    fn from(need: &Need) -> Self {
        Self {
            reason: need.reason,
            required_by: Some(need.required_by.clone()),
        }
    }
}

/// Ce qu'une source permet de faire d'un élément.
enum Offer {
    Change(Box<Change>),
    /// Déjà installé tel quel.
    Current,
    /// Inutilisable, et pourquoi.
    Refused(String),
}

pub(super) struct Walk<'a> {
    context: &'a PlanContext,
    pub selected: Vec<Selected>,
    by_id: HashMap<String, usize>,
    refused: HashSet<String>,
    waiting: VecDeque<Need>,
    pub findings: Findings,
}

impl<'a> Walk<'a> {
    pub fn new(context: &'a PlanContext) -> Self {
        Self {
            context,
            selected: Vec::new(),
            by_id: HashMap::new(),
            refused: HashSet::new(),
            waiting: VecDeque::new(),
            findings: Findings::default(),
        }
    }

    /// `NotFound` si la source, ou l'élément dans cette source, n'existe pas.
    pub fn add_target(&mut self, target: &ItemRef) -> AppResult<()> {
        let context = self.context;
        let source = context.source_named(&target.repository_id);
        let store = match source {
            Some(index) => &context.sources[index],
            None if target.repository_id == LOCAL_SOURCE_ID => &context.local,
            None => return Err(AppError::NotFound),
        };
        let entry = store.catalog.entry(&target.id).ok_or(AppError::NotFound)?;
        match (entry.entry_type, source) {
            (CatalogItemType::Pack, _) => {
                self.add_pack(&target.id, &store.root, source);
                Ok(())
            }
            (kind, Some(source)) if kind.is_installed_by_itself() => {
                self.add_item(source, &target.id)
            }
            _ => Err(AppError::Invalid),
        }
    }

    fn add_pack(&mut self, id: &str, root: &Path, source: Option<usize>) {
        let pack = match read_pack_installation(&[root.to_path_buf()], id) {
            Ok(pack) => pack,
            Err(error) => return self.refuse(id, message_of(error, id)),
        };
        let requester = Requester { id, source };
        let components = pack
            .components
            .into_iter()
            .chain(pack.entrypoint)
            .filter(|component| component.kind.is_installed_by_itself());
        self.waiting.extend(components.map(|component| Need {
            minimum: Some(component.version),
            reason: PlanReason::Component,
            ..requester.needs(component.id, Strictness::Required)
        }));
    }

    fn add_item(&mut self, source: usize, id: &str) -> AppResult<()> {
        let context = self.context;
        let origin = &context.sources[source].origin;
        let place = Place {
            reason: PlanReason::Target,
            required_by: None,
        };
        let holder = match self.by_id.get(id) {
            Some(known) => self.selected[*known].origin.clone(),
            None => match context.standing(id) {
                Standing::Absent => {
                    let offer = self.install(source, id)?;
                    self.take_for_target(id, offer, place);
                    return Ok(());
                }
                Standing::Provided { origin, .. } => origin,
                Standing::Installed {
                    record, is_damaged, ..
                } => {
                    if record.origin.is_same_as(origin) {
                        let offer = self.refresh(source, record, is_damaged)?;
                        self.take_for_target(id, offer, place);
                        return Ok(());
                    }
                    record.origin.clone()
                }
            },
        };
        // Un seul homonyme est installable ; une cible répétée ne compte qu'une fois.
        if !holder.is_same_as(origin) {
            self.findings.block(PlanBlocker::HeldByOther {
                id: id.to_owned(),
                origin: holder,
            });
        }
        Ok(())
    }

    fn take_for_target(&mut self, id: &str, offer: Offer, place: Place) {
        match offer {
            Offer::Change(change) => self.put(*change, place),
            Offer::Current => self.keep(id, place),
            Offer::Refused(message) => self.refuse(id, message),
        }
    }

    /// Suit tout ce qui reste à satisfaire.
    pub fn run(&mut self) -> AppResult<()> {
        while let Some(need) = self.waiting.pop_front() {
            self.settle(need)?;
        }
        Ok(())
    }

    /// Un nouveau flux a besoin d'une sonde primaire et d'une opération par défaut.
    pub fn add_baseline(&mut self) -> AppResult<()> {
        let flow = self
            .selected
            .iter()
            .find(|item| item.kind == CatalogItemType::Flow && item.change.is_some());
        let Some(flow) = flow else {
            return Ok(());
        };
        let requester_id = flow.id.clone();
        let source = flow.change.as_ref().map(|change| change.source);
        let after = engine_after(self.context, &self.selected);
        let probe = (!after.has_primary_probe()).then(|| {
            self.context.find_in_sources(source, |catalog| {
                owner(catalog, catalog.operations.primary_probe()?)
            })
        });
        let default = after
            .operations
            .default_fallback_operation()
            .is_none()
            .then(|| {
                self.context.find_in_sources(source, |catalog| {
                    owner(catalog, catalog.operations.default_fallback_operation()?)
                })
            });
        let requester = Requester {
            id: &requester_id,
            source,
        };
        for id in [probe, default].into_iter().flatten().flatten() {
            self.waiting.push_back(Need {
                reason: PlanReason::Baseline,
                ..requester.needs(id, Strictness::Required)
            });
        }
        self.run()
    }

    fn settle(&mut self, need: Need) -> AppResult<()> {
        if self.refused.contains(&need.id) {
            return Ok(());
        }
        let context = self.context;
        let minimum = need.minimum.as_deref();
        if let Some(&known) = self.by_id.get(&need.id) {
            let selected = &mut self.selected[known];
            if !selected.required_by.contains(&need.required_by) {
                selected.required_by.push(need.required_by.clone());
            }
            let version = selected.effective_version.clone();
            if satisfies(minimum, &version) {
                return Ok(());
            }
            let is_kept = selected.change.is_none();
            let upgrade = if is_kept { self.upgrade(&need)? } else { None };
            match upgrade {
                Some(change) => self.put(change, Place::from(&need)),
                None => self.report_unsuitable(&need, &version),
            }
            return Ok(());
        }
        match context.standing(&need.id) {
            Standing::Absent => return self.settle_absent(&need),
            Standing::Installed {
                version,
                is_damaged,
                ..
            } => {
                if is_damaged || !satisfies(minimum, &version) {
                    if let Some(change) = self.upgrade(&need)? {
                        self.put(change, Place::from(&need));
                        return Ok(());
                    }
                    if is_damaged {
                        self.report_absent(&need);
                    } else {
                        self.report_unsuitable(&need, &version);
                    }
                }
            }
            Standing::Provided { version, .. } => {
                if !satisfies(minimum, &version) {
                    self.report_unsuitable(&need, &version);
                }
            }
        }
        self.keep(&need.id, Place::from(&need));
        self.warn_of_other_origin(&need);
        Ok(())
    }

    fn settle_absent(&mut self, need: &Need) -> AppResult<()> {
        let context = self.context;
        let candidates = context.candidates(&need.id, need.source);
        let published = |source: &usize| context.published_version(*source, &need.id);
        let suitable = candidates.iter().copied().find(|source| {
            published(source).is_some_and(|version| satisfies(need.minimum.as_deref(), &version))
        });
        match (suitable, candidates.first().and_then(published)) {
            (Some(source), _) => match self.install(source, &need.id)? {
                Offer::Change(change) => self.put(*change, Place::from(need)),
                Offer::Refused(message) => self.report_invalid(need, message),
                Offer::Current => {}
            },
            (None, Some(version)) => self.report_unsuitable(need, &version),
            (None, None) => self.report_absent(need),
        }
        Ok(())
    }

    /// Mise à jour ou réparation d'un élément installé qui satisferait cette exigence.
    fn upgrade(&mut self, need: &Need) -> AppResult<Option<Change>> {
        let context = self.context;
        let Standing::Installed {
            record, is_damaged, ..
        } = context.standing(&need.id)
        else {
            return Ok(None);
        };
        let Some(source) = context.source_of(&record.origin) else {
            return Ok(None);
        };
        match self.refresh(source, record, is_damaged)? {
            Offer::Change(change)
                if satisfies(need.minimum.as_deref(), &change.declared.version) =>
            {
                return Ok(Some(*change));
            }
            Offer::Refused(message) => self.report_invalid(need, message),
            _ => {}
        }
        Ok(None)
    }

    fn install(&self, source: usize, id: &str) -> AppResult<Offer> {
        Ok(match self.fetch(source, id, None)? {
            Ok((content, declared)) => Offer::Change(Box::new(Change {
                source,
                action: PlanAction::Install,
                content,
                declared,
                previous: None,
            })),
            Err(message) => Offer::Refused(message),
        })
    }

    /// Ce que cette source, son origine, permet de faire d'un élément installé.
    fn refresh(
        &self,
        source: usize,
        record: &InstallationRecord,
        is_damaged: bool,
    ) -> AppResult<Offer> {
        let context = self.context;
        let (content, declared) = match self.fetch(source, &record.item_id, Some(record))? {
            Ok(fetched) => fetched,
            Err(message) => return Ok(Offer::Refused(message)),
        };
        let frozen = context.frozen_content(record);
        let action = if is_damaged && record.content_sha256 == content.content_sha256 {
            PlanAction::Repair
        } else if is_damaged
            || available_update(
                record,
                &content,
                frozen.as_ref(),
                context.requires_immutable,
            )
            .is_some()
        {
            PlanAction::Update
        } else {
            return Ok(Offer::Current);
        };
        let version = match context.standing(&record.item_id) {
            Standing::Installed { version, .. } => version,
            _ => record.version.clone(),
        };
        Ok(Offer::Change(Box::new(Change {
            source,
            action,
            content,
            declared,
            previous: Some(Previous {
                version,
                copy: frozen.and_then(|content| {
                    let declared = Declared::of(&content).ok()?;
                    Some((content, declared))
                }),
            }),
        })))
    }

    /// Contenu publié ; `Err` explique pourquoi il n'est pas utilisable.
    fn fetch(
        &self,
        source: usize,
        id: &str,
        known: Option<&InstallationRecord>,
    ) -> AppResult<Result<(ItemContent, Declared), String>> {
        let content = match self.context.published_content(source, id) {
            Ok(content) => content,
            Err(AppError::Storage(error)) => return Err(error.into()),
            Err(error) => return Ok(Err(message_of(error, id))),
        };
        Ok(inspect(&content, known).map(|declared| (content, declared)))
    }

    fn refuse(&mut self, id: &str, message: String) {
        self.refused.insert(id.to_owned());
        self.findings.block(PlanBlocker::InvalidItem {
            id: id.to_owned(),
            message,
        });
    }

    /// Place un changement, et suit ce qu'il exige. Il remplace l'élément gardé de même identifiant.
    fn put(&mut self, change: Change, place: Place) {
        let context = self.context;
        let entry = &change.content.entry;
        let requester = Requester {
            id: &entry.id,
            source: Some(change.source),
        };
        self.waiting
            .extend(needs_of(&change.declared, requester, context));
        if let Some(graph) = &change.declared.graph {
            let (needs, unknown) = needs_of_graph(graph, requester, context);
            self.waiting.extend(needs);
            for id in unknown {
                self.findings.block(PlanBlocker::Missing {
                    id,
                    required_by: entry.id.clone(),
                });
            }
        }
        let selected = Selected {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: entry.entry_type,
            version: entry.version.clone(),
            effective_version: change.declared.version.clone(),
            origin: context.sources[change.source].origin.clone(),
            reason: place.reason,
            required_by: place.required_by.into_iter().collect(),
            change: Some(change),
        };
        self.select(selected);
    }

    /// Garde un élément déjà installé, et vérifie que ce dont il se sert est là.
    fn keep(&mut self, id: &str, place: Place) {
        let context = self.context;
        let (origin, version, source) = match context.standing(id) {
            Standing::Installed {
                record, version, ..
            } => (
                record.origin.clone(),
                version,
                context.source_of(&record.origin),
            ),
            Standing::Provided { origin, version } => (origin, version, None),
            Standing::Absent => return,
        };
        let entry = context
            .engine
            .entry(id)
            .cloned()
            .or_else(|| Some(context.frozen_content(context.record(id)?)?.entry));
        let kind = context
            .record(id)
            .map(|record| record.kind)
            .or(entry.as_ref().map(|entry| entry.entry_type));
        let Some(kind) = kind else {
            return;
        };
        let used = elements_declared_by(id, &context.engine);
        let requester = Requester { id, source };
        self.waiting.extend(
            used.items
                .into_iter()
                .chain(used.tools)
                .map(|used| requester.needs(used, Strictness::Inherited)),
        );
        self.select(Selected {
            id: id.to_owned(),
            name: entry.map_or_else(|| id.to_owned(), |entry| entry.name),
            kind,
            version: version.clone(),
            effective_version: version,
            origin,
            reason: place.reason,
            required_by: place.required_by.into_iter().collect(),
            change: None,
        });
    }

    fn select(&mut self, mut selected: Selected) {
        match self.by_id.get(&selected.id) {
            Some(&known) => {
                let kept = &mut self.selected[known];
                selected.reason = kept.reason;
                selected.required_by = std::mem::take(&mut kept.required_by);
                *kept = selected;
            }
            None => {
                self.by_id.insert(selected.id.clone(), self.selected.len());
                self.selected.push(selected);
            }
        }
    }

    fn report_absent(&mut self, need: &Need) {
        match need.strictness {
            Strictness::Required => {
                self.findings.block(PlanBlocker::Missing {
                    id: need.id.clone(),
                    required_by: need.required_by.clone(),
                });
                self.findings.suggest(need.repository.as_ref());
            }
            Strictness::Recommended | Strictness::Optional => self.warn_optional(need),
            Strictness::Inherited => {}
        }
    }

    fn report_invalid(&mut self, need: &Need, message: String) {
        match need.strictness {
            Strictness::Required | Strictness::Recommended => self.refuse(&need.id, message),
            Strictness::Optional => self.warn_optional(need),
            Strictness::Inherited => {}
        }
    }

    fn report_unsuitable(&mut self, need: &Need, available: &str) {
        match (need.strictness, &need.minimum) {
            (Strictness::Required | Strictness::Recommended, Some(required)) => {
                self.findings.block(PlanBlocker::VersionTooOld {
                    id: need.id.clone(),
                    available: available.to_owned(),
                    required: required.clone(),
                });
            }
            (Strictness::Optional, _) => self.warn_optional(need),
            _ => {}
        }
    }

    fn warn_optional(&mut self, need: &Need) {
        self.findings.warn(PlanWarning::OptionalMissing {
            id: need.id.clone(),
            required_by: need.required_by.clone(),
        });
    }

    /// La source qui l'exige publie un homonyme de ce qui est installé.
    fn warn_of_other_origin(&mut self, need: &Need) {
        let context = self.context;
        let Some(kept) = self.by_id.get(&need.id).map(|known| &self.selected[*known]) else {
            return;
        };
        let has_homonym = need.source.is_some_and(|source| {
            let source = &context.sources[source];
            source.catalog.entry(&need.id).is_some() && !source.origin.is_same_as(&kept.origin)
        });
        if has_homonym && need.strictness != Strictness::Inherited {
            let origin = kept.origin.clone();
            self.findings.warn(PlanWarning::OtherOrigin {
                id: need.id.clone(),
                origin,
            });
        }
    }
}

/// Porteur d'une opération.
fn owner(catalog: &EngineCatalog, operation: &crate::flows::OperationContract) -> Option<String> {
    catalog
        .operations
        .owner_of(&operation.id)
        .map(str::to_owned)
}

/// `Err` explique pourquoi ce contenu n'est pas utilisable.
fn inspect(content: &ItemContent, known: Option<&InstallationRecord>) -> Result<Declared, String> {
    let entry = &content.entry;
    if known.is_some_and(|known| known.kind != entry.entry_type) {
        return Err("son dépôt en a changé le type".to_owned());
    }
    if entry.entry_type == CatalogItemType::Script {
        files_of_copy(content).map_err(|error| message_of(error, &entry.id))?;
    }
    Declared::of(content)
}
