//! Notifications toast flottantes : file bornée, fermeture manuelle et expiration automatique.

use super::icon::{Icon, IconName};
use leptos::prelude::*;

const MAX_TOASTS: usize = 4;
const TOAST_DURATION_MS: i32 = 4_000;
const TOAST_ICON_SIZE: u32 = 16;
const CLOSE_ICON_SIZE: u32 = 14;

/// Ton sémantique d'une notification toast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Info,
    Success,
    Danger,
}

impl ToastTone {
    pub const fn as_class(self) -> &'static str {
        match self {
            Self::Info => "toast-item toast-item--info",
            Self::Success => "toast-item toast-item--success",
            Self::Danger => "toast-item toast-item--danger",
        }
    }

    pub const fn icon(self) -> IconName {
        match self {
            Self::Info | Self::Danger => IconName::Alert,
            Self::Success => IconName::Check,
        }
    }

    pub const fn role(self) -> &'static str {
        match self {
            Self::Danger => "alert",
            Self::Info | Self::Success => "status",
        }
    }
}

/// Notification affichée dans la pile flottante.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastEntry {
    pub id: u64,
    pub tone: ToastTone,
    pub message: String,
}

/// File bornée sans DOM : garde au plus `MAX_TOASTS` notifications visibles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToastQueue {
    next_id: u64,
    items: Vec<ToastEntry>,
}

impl ToastQueue {
    pub fn push(&mut self, tone: ToastTone, message: impl Into<String>) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.items.push(ToastEntry {
            id,
            tone,
            message: message.into(),
        });
        if self.items.len() > MAX_TOASTS {
            self.items.remove(0);
        }
        id
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|entry| entry.id != id);
    }

    pub fn entries(&self) -> &[ToastEntry] {
        &self.items
    }
}

/// Contexte réactif des notifications toast de la coque.
#[derive(Clone, Copy)]
pub struct Toasts(RwSignal<ToastQueue>);

impl Toasts {
    pub fn install() -> Self {
        let state = Self(RwSignal::new(ToastQueue::default()));
        provide_context(state);
        state
    }

    pub fn push(self, tone: ToastTone, message: impl Into<String>) {
        let text = message.into();
        if text.trim().is_empty() {
            return;
        }
        let mut id = 0_u64;
        self.0.update(|queue| {
            id = queue.push(tone, text);
        });
        let signal = self.0;
        leptos::task::spawn_local(async move {
            crate::sleep::sleep(TOAST_DURATION_MS).await;
            signal.update(|queue| queue.dismiss(id));
        });
    }

    pub fn dismiss(self, id: u64) {
        self.0.update(|queue| queue.dismiss(id));
    }

    pub fn notify_success(message: impl Into<String>) {
        if let Some(toasts) = use_context::<Self>() {
            toasts.push(ToastTone::Success, message);
        }
    }

    pub fn notify_error(message: impl Into<String>) {
        if let Some(toasts) = use_context::<Self>() {
            toasts.push(ToastTone::Danger, message);
        }
    }

    pub fn notify_info(message: impl Into<String>) {
        if let Some(toasts) = use_context::<Self>() {
            toasts.push(ToastTone::Info, message);
        }
    }

    pub fn set_success(feedback: RwSignal<Option<String>>, message: impl Into<String>) {
        let text = message.into();
        feedback.set(Some(text.clone()));
        Self::notify_success(text);
    }

    pub fn set_error(feedback: RwSignal<Option<String>>, message: impl Into<String>) {
        let text = message.into();
        feedback.set(Some(text.clone()));
        Self::notify_error(text);
    }
}

/// Zone flottante des notifications en bas à droite de la coque.
#[component]
pub fn ToastViewport() -> impl IntoView {
    let toasts = use_context::<Toasts>().unwrap_or_else(Toasts::install);
    view! {
        <div class="toast-stack" aria-live="polite" aria-label="Notifications">
            {move || toasts.0.with(|queue| queue.entries().to_vec()).into_iter().map(|entry| {
                let id = entry.id;
                view! {
                    <div class=entry.tone.as_class() role=entry.tone.role()>
                        <span class="toast-item__icon"><Icon name=entry.tone.icon() size=TOAST_ICON_SIZE /></span>
                        <span class="toast-item__message">{entry.message}</span>
                        <button type="button" class="toast-item__close" aria-label="Fermer la notification"
                            on:click=move |_| toasts.dismiss(id)>
                            <Icon name=IconName::Close size=CLOSE_ICON_SIZE />
                        </button>
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_evicts_oldest_beyond_limit_and_dismisses_by_id() {
        let mut queue = ToastQueue::default();
        let ids: Vec<u64> = (1..=6)
            .map(|n| queue.push(ToastTone::Info, format!("Note {n}")))
            .collect();
        assert_eq!(queue.entries().len(), MAX_TOASTS);
        assert_eq!(queue.entries()[0].id, ids[2]);
        assert_eq!(queue.entries()[MAX_TOASTS - 1].message, "Note 6");

        queue.dismiss(ids[3]);
        assert_eq!(queue.entries().len(), MAX_TOASTS - 1);
        assert!(queue.entries().iter().all(|entry| entry.id != ids[3]));
    }

    #[test]
    fn tone_metadata_covers_every_variant() {
        for tone in [ToastTone::Info, ToastTone::Success, ToastTone::Danger] {
            assert!(tone.as_class().starts_with("toast-item"));
            assert!(!tone.role().is_empty());
            let _ = tone.icon();
        }
    }
}
