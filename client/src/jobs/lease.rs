//! Joignabilité du worker titulaire d'une tâche (ADR 0027) : libellé et pastille.

use crate::{
    format,
    ui::{Tag, TagTone},
};
use chrono::{DateTime, Utc};
use forgearr_shared::jobs::{JobLeaseInfo, LeaseReachability};
use leptos::prelude::*;
use uuid::Uuid;

/// Route des états de bail, lue avec `?ids=`.
pub const LEASES_PATH: &str = "/api/v1/jobs/leases";

/// Libellé de l'état du bail, quand il mérite d'être signalé.
pub fn lease_label(
    info: &JobLeaseInfo,
    datetime: impl Fn(DateTime<Utc>) -> String,
) -> Option<String> {
    match info.reachability {
        LeaseReachability::Reachable => None,
        LeaseReachability::Unreachable => Some(match info.expires_at {
            Some(at) => format!("Injoignable · reprise le {}", datetime(at)),
            None => "Injoignable".to_owned(),
        }),
        LeaseReachability::AwaitingWorker => {
            Some("Attend son worker désigné (hors ligne)".to_owned())
        }
    }
}

/// Pastille d'état du bail d'une tâche, pour son volet de détails.
#[component]
pub fn LeaseTag(job_id: Uuid) -> impl IntoView {
    let infos = LocalResource::new(move || async move {
        crate::api::get::<Vec<JobLeaseInfo>>(&format!("{LEASES_PATH}?ids={job_id}"))
            .await
            .unwrap_or_default()
    });
    move || {
        infos
            .get()
            .unwrap_or_default()
            .first()
            .and_then(|info| lease_label(info, format::datetime))
            .map(|label| view! { <Tag label=label tone=TagTone::Warning /> })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(reachability: LeaseReachability, expires_at: Option<DateTime<Utc>>) -> JobLeaseInfo {
        JobLeaseInfo {
            job_id: Uuid::nil(),
            reachability,
            unreachable_since: None,
            expires_at,
        }
    }

    #[test]
    fn only_a_lease_in_trouble_is_labelled() {
        let label = |reachability, expires_at| {
            lease_label(&info(reachability, expires_at), |_| "midi".to_owned())
        };
        assert_eq!(label(LeaseReachability::Reachable, None), None);
        assert_eq!(
            label(
                LeaseReachability::Unreachable,
                Some(DateTime::<Utc>::UNIX_EPOCH)
            )
            .as_deref(),
            Some("Injoignable · reprise le midi")
        );
        assert_eq!(
            label(LeaseReachability::Unreachable, None).as_deref(),
            Some("Injoignable")
        );
        assert!(label(LeaseReachability::AwaitingWorker, None).is_some());
    }
}
