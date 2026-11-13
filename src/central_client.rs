//! Client du worker vers les routes de la centrale limitées à son nœud (ADR 0011, phase 3).
//! Liste fermée de routes : jamais un proxy générique.

use crate::{
    node_view::{
        ACCOUNTS_PATH, AccountSnapshot, JOBS_PATH, LOGIN_PATH, NodeOverview, OVERVIEW_PATH,
        PLUGINS_PATH, VerifiedAccount, VerifyLoginRequest,
    },
    plugins::CatalogEntry,
};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq, Eq)]
pub enum CentralError {
    /// La centrale a répondu et refusé (identifiants faux, trop d'essais, clé révoquée…).
    Rejected(StatusCode),
    /// Pas de réponse exploitable : réseau coupé, centrale arrêtée, réponse illisible.
    Unreachable,
}

impl From<CentralError> for crate::error::AppError {
    fn from(_: CentralError) -> Self {
        Self::CentralUnavailable
    }
}

#[derive(Clone)]
pub struct CentralClient {
    client: Client,
    base: Url,
    key: String,
}

impl CentralClient {
    pub fn new(client: Client, base: Url, key: String) -> Self {
        Self { client, base, key }
    }

    fn url(&self, path: &str) -> Result<Url, CentralError> {
        self.base.join(path).map_err(|_| CentralError::Unreachable)
    }

    async fn decode<T: DeserializeOwned>(
        response: Result<reqwest::Response, reqwest::Error>,
    ) -> Result<T, CentralError> {
        let response = response.map_err(|_| CentralError::Unreachable)?;
        let status = response.status();
        if status.is_client_error() {
            return Err(CentralError::Rejected(status));
        }
        if !status.is_success() {
            return Err(CentralError::Unreachable);
        }
        response.json().await.map_err(|_| CentralError::Unreachable)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CentralError> {
        let response = self
            .client
            .get(self.url(path)?)
            .bearer_auth(&self.key)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;
        Self::decode(response).await
    }

    pub async fn overview(&self) -> Result<NodeOverview, CentralError> {
        self.get(OVERVIEW_PATH).await
    }

    /// Tâches du nœud, relayées telles que la centrale les renvoie.
    pub async fn jobs(&self) -> Result<serde_json::Value, CentralError> {
        self.get(JOBS_PATH).await
    }

    pub async fn plugins(&self) -> Result<Vec<CatalogEntry>, CentralError> {
        self.get(PLUGINS_PATH).await
    }

    pub async fn accounts(&self) -> Result<AccountSnapshot, CentralError> {
        self.get(ACCOUNTS_PATH).await
    }

    /// Fait vérifier un identifiant et un mot de passe par la centrale.
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<VerifiedAccount, CentralError> {
        let response = self
            .client
            .post(self.url(LOGIN_PATH)?)
            .bearer_auth(&self.key)
            .timeout(REQUEST_TIMEOUT)
            .json(&VerifyLoginRequest {
                username: username.to_owned(),
                password: password.to_owned(),
            })
            .send()
            .await;
        Self::decode(response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unreachable_client() -> anyhow::Result<CentralClient> {
        Ok(CentralClient::new(
            Client::new(),
            Url::parse("http://127.0.0.1:9")?,
            "fdf_cle".into(),
        ))
    }

    #[tokio::test]
    async fn test_a_silent_central_is_unreachable_not_rejected() -> anyhow::Result<()> {
        let client = unreachable_client()?;
        assert_eq!(client.overview().await, Err(CentralError::Unreachable));
        assert_eq!(
            client.login("admin", "x").await,
            Err(CentralError::Unreachable)
        );
        Ok(())
    }
}
