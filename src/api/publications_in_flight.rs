//! Publications que ce processus du central mène, détachées de la connexion HTTP qui les a demandées
//! (ADR 0027) : une coupure ne les annule pas, une répétition les rejoint.

use crate::{
    error::{AppError, AppResult},
    repository::LeaseKey,
};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};
use tokio::sync::watch;

/// Issue partageable entre plusieurs requêtes en attente.
type Outcome = Option<Result<(), Arc<AppError>>>;

#[derive(Clone, Default)]
pub struct PublicationsInFlight {
    running: Arc<Mutex<HashMap<LeaseKey, watch::Receiver<Outcome>>>>,
}

/// Retire l'inscription à la fin de la publication, même sur une panique.
struct Registration {
    publications: PublicationsInFlight,
    key: LeaseKey,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.publications.lock().remove(&self.key);
    }
}

impl PublicationsInFlight {
    fn lock(&self) -> MutexGuard<'_, HashMap<LeaseKey, watch::Receiver<Outcome>>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn contains(&self, key: LeaseKey) -> bool {
        self.lock().contains_key(&key)
    }

    pub fn keys(&self) -> HashSet<LeaseKey> {
        self.lock().keys().copied().collect()
    }

    /// Lance `publication` si aucune ne court pour ce bail, sinon rejoint celle en cours. Le travail
    /// se poursuit si l'appelant disparaît ; l'inscription précède sa première instruction.
    pub async fn run<F>(&self, key: LeaseKey, publication: F) -> AppResult<()>
    where
        F: Future<Output = AppResult<()>> + Send + 'static,
    {
        let waiting = {
            let mut running = self.lock();
            match running.get(&key) {
                Some(existing) => existing.clone(),
                None => {
                    let (sender, receiver) = watch::channel(None);
                    running.insert(key, receiver.clone());
                    let registration = Registration {
                        publications: self.clone(),
                        key,
                    };
                    tokio::spawn(async move {
                        let outcome = publication.await.map_err(Arc::new);
                        drop(registration);
                        let _ = sender.send(Some(outcome));
                    });
                    receiver
                }
            }
        };
        wait(waiting).await
    }
}

async fn wait(mut waiting: watch::Receiver<Outcome>) -> AppResult<()> {
    let outcome = waiting
        .wait_for(Option::is_some)
        .await
        .map_err(|_| AppError::Internal)?
        .clone();
    match outcome {
        Some(Err(error)) => Err(same_error(&error)),
        Some(Ok(())) => Ok(()),
        None => Err(AppError::Internal),
    }
}

/// Copie d'une erreur de publication pour chaque requête en attente ; les causes non copiables sont
/// déjà journalisées par la publication.
fn same_error(error: &AppError) -> AppError {
    match error {
        AppError::Invalid => AppError::Invalid,
        AppError::Forbidden => AppError::Forbidden,
        AppError::NotFound => AppError::NotFound,
        AppError::Conflict => AppError::Conflict,
        AppError::LeaseLost => AppError::LeaseLost,
        AppError::PublicationPending => AppError::PublicationPending,
        AppError::SourceChanged => AppError::SourceChanged,
        AppError::TooLarge => AppError::TooLarge,
        AppError::CentralUnavailable => AppError::CentralUnavailable,
        AppError::ResultUndeliverable => AppError::ResultUndeliverable,
        other => {
            tracing::error!(error = ?other, "publication interrompue");
            AppError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::oneshot;
    use uuid::Uuid;

    #[tokio::test]
    async fn a_repeated_request_joins_the_running_publication() -> anyhow::Result<()> {
        let publications = PublicationsInFlight::default();
        let key = (Uuid::new_v4(), 1);
        let runs = Arc::new(AtomicUsize::new(0));
        let (release, gate) = oneshot::channel::<()>();
        let (first, count) = (publications.clone(), runs.clone());
        let caller = tokio::spawn(async move {
            first
                .run(key, async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    let _ = gate.await;
                    Err(AppError::ResultUndeliverable)
                })
                .await
        });
        while runs.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        // La connexion de l'appelant est coupée : la publication continue.
        caller.abort();
        let _ = caller.await;
        assert!(publications.contains(key));
        let (joined, count) = (publications.clone(), runs.clone());
        let repeat = tokio::spawn(async move {
            joined
                .run(key, async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
        });
        // La publication ne finit qu'une fois la répétition en attente.
        tokio::task::yield_now().await;
        let _ = release.send(());
        let outcome = repeat.await?;
        assert!(matches!(outcome, Err(AppError::ResultUndeliverable)));
        while publications.contains(key) {
            tokio::task::yield_now().await;
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert!(publications.keys().is_empty());
        Ok(())
    }
}
