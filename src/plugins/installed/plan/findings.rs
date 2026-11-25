//! Ce que le plan a relevé, sans doublon.

use crate::error::AppError;
use forgearr_shared::{
    exposed_variables::RepositoryPointer,
    plugins::store::plan::{PlanBlocker, PlanConsent, PlanWarning},
};

#[derive(Debug, Default)]
pub(super) struct Findings {
    pub blockers: Vec<PlanBlocker>,
    pub consents: Vec<PlanConsent>,
    pub warnings: Vec<PlanWarning>,
    pub suggested_repositories: Vec<RepositoryPointer>,
}

impl Findings {
    pub fn block(&mut self, blocker: PlanBlocker) {
        add(&mut self.blockers, blocker);
    }

    pub fn ask(&mut self, consent: PlanConsent) {
        add(&mut self.consents, consent);
    }

    pub fn warn(&mut self, warning: PlanWarning) {
        add(&mut self.warnings, warning);
    }

    pub fn suggest(&mut self, repository: Option<&RepositoryPointer>) {
        if let Some(repository) = repository {
            add(&mut self.suggested_repositories, repository.clone());
        }
    }
}

fn add<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// Cause lisible d'un élément inutilisable, sans son identifiant que le blocage donne déjà.
pub(super) fn message_of(error: AppError, id: &str) -> String {
    let message = match error {
        AppError::InvalidManifest(message) => message,
        AppError::NotFound => "fichiers absents de sa source".to_owned(),
        other => other.to_string(),
    };
    match message.strip_prefix(&format!("{id} : ")) {
        Some(cause) => cause.to_owned(),
        None => message,
    }
}
