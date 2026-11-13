//! Sessions web d'un worker (ADR 0011, phase 3) : en mémoire seulement, perdues au redémarrage et
//! purgées dès qu'un compte disparaît ou est désactivé à la synchronisation.

use crate::{
    web_session::{ValidatedSession, WebSession, digest},
    worker_view::VerifiedAccount,
};
use chrono::{Duration, Utc};
use std::{collections::HashMap, sync::Mutex};
use uuid::Uuid;

struct Entry {
    session: ValidatedSession,
    account: VerifiedAccount,
}

pub struct SessionStore {
    ttl: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

impl SessionStore {
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            ttl: Duration::seconds(ttl_seconds),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Ouvre une session pour ce compte ; les sessions expirées sont oubliées au passage.
    pub fn open(&self, account: &VerifiedAccount) -> WebSession {
        let web = WebSession {
            token: Uuid::new_v4().to_string(),
            csrf_token: Uuid::new_v4().to_string(),
            expires_at: Utc::now() + self.ttl,
        };
        let token_hash = digest(&web.token);
        if let Ok(mut entries) = self.entries.lock() {
            let now = Utc::now();
            entries.retain(|_, entry| entry.session.expires_at > now);
            entries.insert(
                token_hash.clone(),
                Entry {
                    session: ValidatedSession {
                        token_hash,
                        csrf_hash: digest(&web.csrf_token),
                        expires_at: web.expires_at,
                        account_id: account.id.clone(),
                    },
                    account: account.clone(),
                },
            );
        }
        web
    }

    /// Session encore valable pour cette empreinte de jeton, avec son compte.
    pub fn lookup(&self, token_hash: &str) -> Option<(ValidatedSession, VerifiedAccount)> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(token_hash)?;
        (entry.session.expires_at > Utc::now())
            .then(|| (entry.session.clone(), entry.account.clone()))
    }

    pub fn close(&self, token_hash: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(token_hash);
        }
    }

    /// Ferme les sessions des comptes qui ne figurent plus parmi les comptes actifs.
    pub fn retain_accounts(&self, active_ids: &[String]) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, entry| active_ids.contains(&entry.account.id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::Role;

    fn account(id: &str) -> VerifiedAccount {
        VerifiedAccount {
            id: id.into(),
            username: format!("user-{id}"),
            role: Role::Admin,
        }
    }

    #[test]
    fn test_opened_session_is_found_by_its_token_hash_only() {
        let store = SessionStore::new(60);
        let web = store.open(&account("1"));
        let found = store.lookup(&digest(&web.token));
        assert_eq!(found.map(|(_, account)| account.id), Some("1".to_owned()));
        assert!(store.lookup(&web.token).is_none());
        assert!(store.lookup(&digest("autre")).is_none());
    }

    #[test]
    fn test_closed_and_expired_sessions_are_gone() {
        let store = SessionStore::new(60);
        let web = store.open(&account("1"));
        store.close(&digest(&web.token));
        assert!(store.lookup(&digest(&web.token)).is_none());
        let expired = SessionStore::new(-1);
        let web = expired.open(&account("1"));
        assert!(expired.lookup(&digest(&web.token)).is_none());
    }

    #[test]
    fn test_sessions_of_removed_accounts_are_purged() {
        let store = SessionStore::new(60);
        let kept = store.open(&account("1"));
        let dropped = store.open(&account("2"));
        store.retain_accounts(&["1".to_owned()]);
        assert!(store.lookup(&digest(&kept.token)).is_some());
        assert!(store.lookup(&digest(&dropped.token)).is_none());
    }
}
