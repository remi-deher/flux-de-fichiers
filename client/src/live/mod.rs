//! Événements en temps réel du central (ADR 0028) : un seul abonnement par page, un signal par sujet.

mod changed_elsewhere;
pub(crate) mod reconnect;
mod resource;
mod stream;

use crate::jobs::LiveJobs;
pub use changed_elsewhere::ChangedElsewhere;
use forgearr_shared::InstanceKind;
use forgearr_shared::live::{ChangeTopic, ChangedEvent, JOB_EVENTS_PERMISSION};
use forgearr_shared::session::Me;
use leptos::prelude::*;
pub use resource::{Loaded, load};

/// Sujets changés sur le central, partagés par contexte ; sur un worker, aucun flux ne les anime.
#[derive(Clone, Copy)]
pub struct LiveEvents {
    catalog: RwSignal<u32>,
    tools: RwSignal<u32>,
    repositories: RwSignal<u32>,
    settings: RwSignal<u32>,
}

impl LiveEvents {
    /// Installe le contexte pour la session et, sur le central, ouvre l'unique abonnement.
    pub fn install(me: &Me) {
        let live = Self {
            catalog: RwSignal::new(0),
            tools: RwSignal::new(0),
            repositories: RwSignal::new(0),
            settings: RwSignal::new(0),
        };
        provide_context(live);
        if me.instance_kind != InstanceKind::Central {
            return;
        }
        let jobs = me.allows(JOB_EVENTS_PERMISSION).then(LiveJobs::install);
        if may_listen(me) {
            stream::open(live, jobs);
        }
    }

    const fn signal(self, topic: ChangeTopic) -> RwSignal<u32> {
        match topic {
            ChangeTopic::Catalog => self.catalog,
            ChangeTopic::Tools => self.tools,
            ChangeTopic::Repositories => self.repositories,
            ChangeTopic::Settings => self.settings,
        }
    }

    /// Relit l'appelant à chaque changement d'un de ces sujets.
    pub fn track(self, topics: &[ChangeTopic]) {
        for topic in topics {
            self.signal(*topic).track();
        }
    }

    /// Fait relire ces sujets : événement reçu, ou écriture de cette page sans attendre le flux.
    pub fn touch(self, topics: &[ChangeTopic]) {
        for topic in topics {
            // La session a pu se fermer entre-temps.
            let _ = self
                .signal(*topic)
                .try_update(|count| *count = count.wrapping_add(1));
        }
    }
}

/// Contexte de la session ; à lire dans le corps d'un composant.
pub fn use_live() -> LiveEvents {
    expect_context::<LiveEvents>()
}

/// Le central accepte-t-il d'ouvrir le flux à cet utilisateur ? (mêmes droits que la route)
fn may_listen(me: &Me) -> bool {
    me.instance_kind == InstanceKind::Central
        && (me.allows(JOB_EVENTS_PERMISSION)
            || ChangeTopic::ALL
                .into_iter()
                .flat_map(ChangeTopic::viewers)
                .any(|permission| me.allows(*permission)))
}

/// Sujet d'un événement « changed » ; `None` pour une charge illisible ou un sujet inconnu.
fn changed_topic(data: &str) -> Option<ChangeTopic> {
    serde_json::from_str::<ChangedEvent>(data)
        .ok()
        .map(|event| event.topic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::Role;
    use forgearr_shared::session::CallerKind;

    fn me(role: Role, kind: InstanceKind) -> Me {
        Me::new(CallerKind::Account, "compte".into(), role, kind, None)
    }

    #[test]
    fn the_stream_opens_only_on_a_central_for_who_can_read_something() {
        assert!(may_listen(&me(Role::Admin, InstanceKind::Central)));
        assert!(may_listen(&me(Role::Viewer, InstanceKind::Central)));
        assert!(!may_listen(&me(Role::Admin, InstanceKind::Worker)));
        let mut powerless = me(Role::Viewer, InstanceKind::Central);
        powerless.permissions.clear();
        assert!(!may_listen(&powerless));
    }

    #[test]
    fn an_unknown_topic_or_a_broken_payload_is_ignored() {
        assert_eq!(
            changed_topic(r#"{"topic":"tools"}"#),
            Some(ChangeTopic::Tools)
        );
        assert_eq!(changed_topic(r#"{"topic":"inconnu"}"#), None);
        assert_eq!(changed_topic(""), None);
    }
}
