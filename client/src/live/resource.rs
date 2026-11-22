//! Lecture d'une route relue à chaque changement de ses sujets, sans clignotement.

use super::use_live;
use crate::api::{self, ApiError};
use forgearr_shared::live::ChangeTopic;
use leptos::prelude::*;
use serde::de::DeserializeOwned;

/// Dernier contenu connu : `None` avant la première réponse, puis conservé pendant les relectures.
pub type Loaded<T> = Memo<Option<Result<T, ApiError>>>;

const SERVER_ERROR_STATUS: u16 = 500;

/// Panne passagère : elle ne dit rien du contenu déjà lu.
fn is_transient(error: &ApiError) -> bool {
    match error {
        ApiError::Network => true,
        ApiError::Rejected { status, .. } => *status >= SERVER_ERROR_STATUS,
        ApiError::Unauthorized => false,
    }
}

/// Contenu à montrer après une lecture : une panne passagère garde le contenu précédent.
fn settle<T: Clone>(
    previous: Option<&Result<T, ApiError>>,
    fresh: Option<Result<T, ApiError>>,
) -> Option<Result<T, ApiError>> {
    match (previous, fresh) {
        (Some(Ok(known)), Some(Err(error))) if is_transient(&error) => Some(Ok(known.clone())),
        (_, Some(fresh)) => Some(fresh),
        (previous, None) => previous.cloned(),
    }
}

/// Lit `path` et le relit à chaque changement de `topics`. Un contenu identique ne notifie rien.
pub fn load<T>(topics: &'static [ChangeTopic], path: impl Into<String>) -> Loaded<T>
where
    T: DeserializeOwned + Clone + PartialEq + Send + Sync + 'static,
{
    let live = use_live();
    let path = path.into();
    let resource = LocalResource::new(move || {
        live.track(topics);
        let path = path.clone();
        async move { api::get::<T>(&path).await }
    });
    Memo::new(move |previous: Option<&Option<Result<T, ApiError>>>| {
        settle(previous.and_then(Option::as_ref), resource.get())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(status: u16) -> ApiError {
        ApiError::Rejected {
            status,
            code: "x".into(),
            detail: None,
        }
    }

    #[test]
    fn a_transient_failure_keeps_the_known_content() {
        let known = Ok(1);
        for error in [ApiError::Network, rejected(502)] {
            assert_eq!(settle(Some(&known), Some(Err(error))), Some(Ok(1)));
        }
        assert_eq!(settle(Some(&known), None), Some(Ok(1)));
        assert_eq!(settle(Some(&known), Some(Ok(2))), Some(Ok(2)));
    }

    #[test]
    fn a_real_refusal_and_a_first_failure_are_shown() {
        assert_eq!(
            settle(Some(&Ok(1)), Some(Err(rejected(404)))),
            Some(Err(rejected(404)))
        );
        assert_eq!(
            settle::<u8>(None, Some(Err(ApiError::Network))),
            Some(Err(ApiError::Network))
        );
        assert_eq!(settle::<u8>(None, None), None);
    }
}
