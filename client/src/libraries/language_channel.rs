//! Canal des suggestions du serveur de langage (ADR 0032, lot b2) : un WebSocket par éditeur, ouvert avec lui
//! et fermé avec lui. Un canal fermé se rouvre de lui-même, avec une attente qui double à chaque échec, la
//! prochaine fois que l'éditeur en a besoin ; en attendant, la liste locale (a) répond seule.

use super::language_logic::{Link, is_served, language_position, session_url};
use forgearr_shared::{
    language::{ClientMessage, Diagnostic, ServerMessage, Suggestion},
    local::ScriptInterpreter,
};
use leptos::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{MessageEvent, WebSocket};

/// Ce que l'éditeur fait d'une réponse du serveur ; jamais appelé si le canal tombe avant.
type Reply = Box<dyn FnOnce(Vec<Suggestion>)>;

/// Dernières anomalies publiées par le serveur, avec le texte pour lequel il les a calculées.
pub type Diagnosed = Option<(String, Vec<Diagnostic>)>;

/// Canal d'un éditeur ; le cloner donne un autre accès au même canal.
#[derive(Clone)]
pub struct LanguageChannel(Rc<Channel>);

struct Channel {
    interpreter: Cell<ScriptInterpreter>,
    link: Cell<Link>,
    socket: RefCell<Option<WebSocket>>,
    pending: RefCell<HashMap<u32, Reply>>,
    next_id: Cell<u32>,
    /// Version du texte connu du serveur, à partir de 1 après son ouverture.
    version: Cell<u32>,
    /// Texte que le serveur a reçu en dernier (ouverture ou changement).
    synced: RefCell<Option<String>>,
    /// Texte le plus récent vu par le canal, envoyé à l'ouverture du document.
    latest: RefCell<String>,
    diagnostics: RwSignal<Diagnosed>,
    on_message: Closure<dyn FnMut(MessageEvent)>,
    on_lost: Closure<dyn FnMut()>,
}

impl LanguageChannel {
    /// Prépare le canal ; `text` est le code déjà présent et `diagnostics` reçoit ce que le serveur publie.
    /// Rien n'est ouvert avant `set_interpreter`.
    pub fn new(interpreter: ScriptInterpreter, text: &str, diagnostics: RwSignal<Diagnosed>) -> Self {
        let channel = Rc::new_cyclic(|channel: &Weak<Channel>| {
            Channel::new(interpreter, text, diagnostics, channel)
        });
        Self(channel)
    }

    /// Le texte de l'éditeur a changé : le serveur le reçoit (et publie ses diagnostics) si le canal est prêt,
    /// sinon il le recevra à l'ouverture.
    pub fn follow(&self, text: &str) {
        let channel = &self.0;
        channel.latest.replace(text.to_owned());
        if channel.link.get().is_ready() {
            channel.sync(text);
        } else {
            channel.connect_if_due();
        }
    }

    /// Change d'interpréteur : l'ancienne session est fermée, une nouvelle s'ouvre si un serveur existe.
    pub fn set_interpreter(&self, interpreter: ScriptInterpreter) {
        let channel = &self.0;
        if channel.interpreter.get() == interpreter && channel.link.get() != Link::Idle {
            return;
        }
        channel.shut();
        channel.interpreter.set(interpreter);
        channel.link.set(if is_served(interpreter) {
            Link::Idle
        } else {
            Link::Local
        });
        channel.connect_if_due();
    }

    /// Demande les suggestions à `caret` (unités UTF-16) dans `text`. Rend `false` si le canal n'est pas prêt :
    /// l'éditeur s'en tient alors à sa liste locale, et le canal se rouvre si son attente est écoulée.
    pub fn complete(
        &self,
        text: &str,
        caret: usize,
        reply: impl FnOnce(Vec<Suggestion>) + 'static,
    ) -> bool {
        let channel = &self.0;
        channel.latest.replace(text.to_owned());
        if !channel.link.get().is_ready() {
            channel.connect_if_due();
            return false;
        }
        channel.sync(text);
        let id = channel.next_id.get();
        channel.next_id.set(id.wrapping_add(1));
        channel.pending.borrow_mut().insert(id, Box::new(reply));
        channel.send(&ClientMessage::Complete {
            id,
            position: language_position(text, caret),
        });
        true
    }

    /// Ferme la session : le serveur s'arrête avec elle.
    pub fn close(&self) {
        self.0.send(&ClientMessage::Close);
        self.0.shut();
    }
}

impl Channel {
    fn new(
        interpreter: ScriptInterpreter,
        text: &str,
        diagnostics: RwSignal<Diagnosed>,
        channel: &Weak<Self>,
    ) -> Self {
        let message_target = channel.clone();
        let lost_target = channel.clone();
        Self {
            interpreter: Cell::new(interpreter),
            link: Cell::new(Link::Idle),
            socket: RefCell::new(None),
            pending: RefCell::new(HashMap::new()),
            next_id: Cell::new(0),
            version: Cell::new(0),
            synced: RefCell::new(None),
            latest: RefCell::new(text.to_owned()),
            diagnostics,
            on_message: Closure::new(move |event: MessageEvent| {
                if let (Some(channel), Some(data)) =
                    (message_target.upgrade(), event.data().as_string())
                {
                    channel.received(&data);
                }
            }),
            on_lost: Closure::new(move || {
                if let Some(channel) = lost_target.upgrade() {
                    channel.lost();
                }
            }),
        }
    }

    fn connect_if_due(self: &Rc<Self>) {
        if self.link.get().may_connect(js_sys::Date::now()) {
            self.connect();
        }
    }

    fn connect(self: &Rc<Self>) {
        let Some(url) = page_session_url(self.interpreter.get()) else {
            self.link.set(Link::Local);
            return;
        };
        self.link.set(self.link.get().connecting());
        match WebSocket::new(&url) {
            Ok(socket) => {
                socket.set_onmessage(Some(self.on_message.as_ref().unchecked_ref()));
                socket.set_onclose(Some(self.on_lost.as_ref().unchecked_ref()));
                socket.set_onerror(Some(self.on_lost.as_ref().unchecked_ref()));
                self.socket.replace(Some(socket));
            }
            Err(_) => self.lost(),
        }
    }

    /// Message du central. Les info-bulles sont l'affaire du lot c2.
    fn received(&self, data: &str) {
        let Ok(message) = serde_json::from_str::<ServerMessage>(data) else {
            return;
        };
        match message {
            ServerMessage::Ready => {
                self.link.set(Link::Ready);
                self.open_document();
            }
            ServerMessage::Completion { id, items } => {
                let reply = self.pending.borrow_mut().remove(&id);
                if let Some(reply) = reply {
                    reply(items);
                }
            }
            ServerMessage::Unavailable { reason } => {
                self.link
                    .set(self.link.get().unavailable(reason, js_sys::Date::now()));
                self.release();
            }
            ServerMessage::Diagnostics { items } => {
                let known = self.synced.borrow().clone();
                if let Some(text) = known {
                    let _ = self.diagnostics.try_set(Some((text, items)));
                }
            }
            ServerMessage::Hover { .. } => {}
        }
    }

    /// Le canal est tombé : une nouvelle tentative viendra après l'attente.
    fn lost(&self) {
        if matches!(self.link.get(), Link::Connecting { .. } | Link::Ready) {
            self.link.set(self.link.get().dropped(js_sys::Date::now()));
        }
        self.release();
    }

    fn open_document(&self) {
        let text = self.latest.borrow().clone();
        self.version.set(0);
        self.synced.replace(Some(text.clone()));
        self.send(&ClientMessage::Open { text });
    }

    /// Le serveur connaît `text` ; un changement n'est envoyé que s'il en diffère.
    fn sync(&self, text: &str) {
        if self.synced.borrow().as_deref() == Some(text) {
            return;
        }
        let version = self.version.get().saturating_add(1);
        self.version.set(version);
        self.synced.replace(Some(text.to_owned()));
        self.send(&ClientMessage::Change {
            version,
            text: text.to_owned(),
        });
    }

    fn send(&self, message: &ClientMessage) {
        let socket = self.socket.borrow();
        let (Some(socket), Ok(text)) = (socket.as_ref(), serde_json::to_string(message)) else {
            return;
        };
        let _ = socket.send_with_str(&text);
    }

    /// Détache et ferme le socket ; les réponses attendues sont abandonnées.
    fn release(&self) {
        self.pending.borrow_mut().clear();
        let _ = self.diagnostics.try_set(None);
        if let Some(socket) = self.socket.take() {
            socket.set_onmessage(None);
            socket.set_onclose(None);
            socket.set_onerror(None);
            let _ = socket.close();
        }
    }

    /// Ferme sans rouvrir : le lien repart de zéro au prochain réglage.
    fn shut(&self) {
        self.release();
        self.synced.replace(None);
        self.link.set(Link::Local);
    }
}

/// Adresse du canal d'après l'adresse de la page.
fn page_session_url(interpreter: ScriptInterpreter) -> Option<String> {
    let location = web_sys::window()?.location();
    let secure = location.protocol().ok()? == "https:";
    session_url(secure, &location.host().ok()?, interpreter)
}
