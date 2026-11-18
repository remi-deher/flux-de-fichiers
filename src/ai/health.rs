//! État du fournisseur IA (dernier succès, dernière erreur) et cache de la liste des modèles.

use chrono::{SecondsFormat, Utc};
use flux_shared::ai::AiHealthError;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

/// Durée pendant laquelle le premier modèle proposé reste valable avant une nouvelle lecture.
const MODEL_CACHE_TTL: Duration = Duration::from_secs(600);
const MAX_ERROR_CHARS: usize = 300;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Default)]
struct Observed {
    last_success: Option<(String, u64)>,
    last_error: Option<AiHealthError>,
}

/// Dernier succès et dernière erreur des appels au fournisseur, depuis le démarrage.
#[derive(Clone, Default)]
pub struct AiHealthTracker {
    inner: Arc<Mutex<Observed>>,
}

impl AiHealthTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_success(&self, latency_ms: u64) {
        lock(&self.inner).last_success = Some((now(), latency_ms));
    }

    pub fn record_failure(&self, message: &str) {
        let message: String = message.chars().take(MAX_ERROR_CHARS).collect();
        lock(&self.inner).last_error = Some(AiHealthError { at: now(), message });
    }

    /// `(dernier succès : date et latence, dernière erreur)`.
    pub fn snapshot(&self) -> (Option<(String, u64)>, Option<AiHealthError>) {
        let observed = lock(&self.inner);
        (observed.last_success.clone(), observed.last_error.clone())
    }
}

/// Premier modèle proposé par fournisseur et compte, gardé quelques minutes pour ne pas
/// interroger `/models` à chaque question.
#[derive(Clone, Default)]
pub struct ModelCache {
    inner: Arc<Mutex<HashMap<String, (Instant, String)>>>,
}

impl ModelCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        lock(&self.inner)
            .get(key)
            .filter(|(stored, _)| stored.elapsed() < MODEL_CACHE_TTL)
            .map(|(_, model)| model.clone())
    }

    pub fn put(&self, key: &str, model: &str) {
        lock(&self.inner).insert(key.to_owned(), (Instant::now(), model.to_owned()));
    }

    /// À appeler quand les réglages ou la connexion changent.
    pub fn clear(&self) {
        lock(&self.inner).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_keeps_the_latest_success_and_error_independently() {
        let tracker = AiHealthTracker::new();
        assert_eq!(tracker.snapshot(), (None, None));
        tracker.record_success(120);
        tracker.record_failure(&"x".repeat(MAX_ERROR_CHARS * 2));
        let (success, error) = tracker.snapshot();
        assert_eq!(success.map(|(_, latency)| latency), Some(120));
        let error = error.map(|e| e.message.chars().count());
        assert_eq!(error, Some(MAX_ERROR_CHARS));
        tracker.record_success(80);
        assert_eq!(tracker.snapshot().0.map(|(_, l)| l), Some(80));
        assert!(tracker.snapshot().1.is_some());
    }

    #[test]
    fn the_model_cache_serves_until_cleared() {
        let cache = ModelCache::new();
        assert_eq!(cache.get("k"), None);
        cache.put("k", "gpt-a");
        assert_eq!(cache.get("k").as_deref(), Some("gpt-a"));
        assert_eq!(cache.get("autre"), None);
        cache.clear();
        assert_eq!(cache.get("k"), None);
    }
}
