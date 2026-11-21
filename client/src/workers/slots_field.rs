//! Places CPU/GPU d'un worker : champ facultatif et libellé de capacité.

use forgearr_shared::workers::{WORKER_MAX_CLASS_SLOTS, WORKER_MIN_CLASS_SLOTS, Worker};
use leptos::prelude::*;

/// Saisie → places de la classe ; vide (ou illisible) : non déclarée.
pub fn parse_optional_slots(raw: &str) -> Option<i32> {
    raw.trim()
        .parse::<i32>()
        .ok()
        .map(|slots| slots.clamp(WORKER_MIN_CLASS_SLOTS, WORKER_MAX_CLASS_SLOTS))
}

/// Capacité affichée : places par classe si déclarées, sinon concurrence commune.
pub fn capacity_label(worker: &Worker) -> String {
    let slots = worker.slots();
    if worker.max_cpu_jobs.is_none() && worker.max_gpu_jobs.is_none() {
        format!("{} tâche(s), CPU ou GPU", slots.total)
    } else {
        format!("{} CPU · {} GPU ({} au total)", slots.cpu, slots.gpu, slots.total)
    }
}

/// Champ numérique facultatif des places d'une classe (vide = mode historique).
#[component]
pub fn ClassSlotsField(
    input_id: &'static str,
    label: &'static str,
    help: &'static str,
    value: RwSignal<Option<i32>>,
) -> impl IntoView {
    view! {
        <div class="form-group">
            <label class="form-label" for=input_id>{label}</label>
            <input id=input_id type="number" class="form-input"
                min=WORKER_MIN_CLASS_SLOTS max=WORKER_MAX_CLASS_SLOTS
                placeholder="Concurrence commune"
                prop:value=move || value.get().map(|slots| slots.to_string()).unwrap_or_default()
                on:input=move |ev| value.set(parse_optional_slots(&event_target_value(&ev))) />
            <p class="form-help">{help}</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    fn worker(cpu: Option<i32>, gpu: Option<i32>) -> Worker {
        Worker {
            id: Uuid::nil(),
            name: "nas".into(),
            last_seen: DateTime::<Utc>::UNIX_EPOCH,
            version: None,
            remote_addr: None,
            telemetry: None,
            max_concurrency: 2,
            max_cpu_jobs: cpu,
            max_gpu_jobs: gpu,
            schedule: None,
            transfer_enabled: false,
        }
    }

    #[test]
    fn empty_input_keeps_the_legacy_mode_and_values_are_bounded() {
        assert_eq!(parse_optional_slots(""), None);
        assert_eq!(parse_optional_slots(" 3 "), Some(3));
        assert_eq!(parse_optional_slots("99"), Some(WORKER_MAX_CLASS_SLOTS));
        assert_eq!(parse_optional_slots("-1"), Some(WORKER_MIN_CLASS_SLOTS));
    }

    #[test]
    fn capacity_label_follows_the_slot_mode() {
        assert_eq!(capacity_label(&worker(None, None)), "2 tâche(s), CPU ou GPU");
        assert_eq!(capacity_label(&worker(Some(2), Some(1))), "2 CPU · 1 GPU (3 au total)");
    }
}
