//! Abonnement à `/api/v1/events`, tenu toute la session et fermé avec elle.

use super::reconnect::{Reopen, must_resync, retry_delay};
use super::{LiveEvents, changed_topic};
use crate::jobs::{LiveJobs, model::EVENT_NAMES};
use forgearr_shared::jobs::JobEvent;
use forgearr_shared::live::{CHANGED_EVENT, ChangeTopic, EVENTS_PATH, RESYNC_EVENT};
use leptos::leptos_dom::helpers::TimeoutHandle;
use leptos::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{EventSource, MessageEvent};

type Listener = Closure<dyn FnMut(MessageEvent)>;

/// Ouvre le flux jusqu'à la fin de la session courante.
pub fn open(live: LiveEvents, jobs: Option<LiveJobs>) {
    let stream = Rc::new_cyclic(|stream| Stream::new(live, jobs, stream));
    stream.connect();
    let stream = StoredValue::new_local(stream);
    on_cleanup(move || {
        stream.try_with_value(|stream| stream.close());
    });
}

/// Tout relire une fois : des événements ont été manqués.
fn resync(live: LiveEvents, jobs: Option<LiveJobs>) {
    live.touch(&ChangeTopic::ALL);
    if let Some(jobs) = jobs {
        jobs.resync();
    }
}

struct Stream {
    live: LiveEvents,
    jobs: Option<LiveJobs>,
    listeners: Vec<(&'static str, Listener)>,
    on_open: Closure<dyn FnMut()>,
    on_error: Closure<dyn FnMut()>,
    source: RefCell<Option<EventSource>>,
    retry: RefCell<Option<TimeoutHandle>>,
    failures: Cell<u32>,
    /// Des événements ont pu être manqués sans `Last-Event-ID` pour les retrouver.
    lost: Cell<bool>,
    /// Le flux s'est déjà ouvert : le navigateur connaît une position de reprise.
    opened_once: Cell<bool>,
    started_at_ms: f64,
    closed: Cell<bool>,
}

impl Stream {
    fn new(live: LiveEvents, jobs: Option<LiveJobs>, stream: &Weak<Self>) -> Self {
        let on = |stream: &Weak<Self>, react: fn(&Rc<Self>)| {
            let stream = stream.clone();
            Closure::<dyn FnMut()>::new(move || {
                if let Some(stream) = stream.upgrade() {
                    react(&stream);
                }
            })
        };
        Self {
            live,
            jobs,
            listeners: listeners(live, jobs),
            on_open: on(stream, Self::opened),
            on_error: on(stream, Self::failed),
            source: RefCell::new(None),
            retry: RefCell::new(None),
            failures: Cell::new(0),
            lost: Cell::new(false),
            opened_once: Cell::new(false),
            started_at_ms: js_sys::Date::now(),
            closed: Cell::new(false),
        }
    }

    fn connect(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        let Ok(source) = EventSource::new(EVENTS_PATH) else {
            self.lose();
            return;
        };
        for (name, listener) in &self.listeners {
            let _ =
                source.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        source.set_onopen(Some(self.on_open.as_ref().unchecked_ref()));
        source.set_onerror(Some(self.on_error.as_ref().unchecked_ref()));
        self.source.replace(Some(source));
    }

    fn opened(self: &Rc<Self>) {
        self.failures.set(0);
        let first = !self.opened_once.replace(true);
        let first_wait_ms = first.then(|| js_sys::Date::now() - self.started_at_ms);
        if must_resync(self.lost.replace(false), first_wait_ms) {
            resync(self.live, self.jobs);
        }
    }

    /// Une coupure passagère se reconnecte seule ; seul un flux abandonné est rouvert ici.
    fn failed(self: &Rc<Self>) {
        if !self.opened_once.get() {
            self.lost.set(true);
        }
        let abandoned = self
            .source
            .borrow()
            .as_ref()
            .is_some_and(|source| source.ready_state() == EventSource::CLOSED);
        if abandoned {
            self.lose();
        }
    }

    fn lose(self: &Rc<Self>) {
        self.release();
        self.lost.set(true);
        self.retry_later();
    }

    fn retry_later(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        let failures = self.failures.get();
        self.failures.set(failures.saturating_add(1));
        let stream = Rc::downgrade(self);
        let attempt = move || {
            if let Some(stream) = stream.upgrade() {
                leptos::task::spawn_local(async move { stream.reopen().await });
            }
        };
        self.retry
            .replace(set_timeout_with_handle(attempt, retry_delay(failures)).ok());
    }

    async fn reopen(self: Rc<Self>) {
        if self.closed.get() {
            return;
        }
        match Reopen::after(&crate::access::identity().await) {
            Reopen::Now => self.connect(),
            Reopen::Later => self.retry_later(),
            Reopen::Never => {}
        }
    }

    fn release(&self) {
        if let Some(source) = self.source.take() {
            source.set_onopen(None);
            source.set_onerror(None);
            source.close();
        }
    }

    fn close(&self) {
        self.closed.set(true);
        if let Some(retry) = self.retry.take() {
            retry.clear();
        }
        self.release();
    }
}

/// Une réaction par nom d'événement ; un message sans texte est ignoré.
fn listener(react: impl Fn(&str) + 'static) -> Listener {
    Closure::new(move |message: MessageEvent| {
        if let Some(data) = message.data().as_string() {
            react(&data);
        }
    })
}

fn listeners(live: LiveEvents, jobs: Option<LiveJobs>) -> Vec<(&'static str, Listener)> {
    let mut all = vec![
        (
            CHANGED_EVENT,
            listener(move |data| {
                if let Some(topic) = changed_topic(data) {
                    live.touch(&[topic]);
                }
            }),
        ),
        (RESYNC_EVENT, listener(move |_| resync(live, jobs))),
    ];
    if let Some(jobs) = jobs {
        all.extend(EVENT_NAMES.into_iter().map(|name| {
            let react = listener(move |data| {
                if let Ok(event) = serde_json::from_str::<JobEvent>(data) {
                    jobs.apply(&event);
                }
            });
            (name.as_str(), react)
        }));
    }
    all
}
