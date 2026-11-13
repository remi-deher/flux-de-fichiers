//! Copie locale des comptes d'un worker (ADR 0011, phase 3) : elle ne sert qu'à se connecter quand
//! la centrale est injoignable. Elle est remplacée en bloc à chaque synchronisation, suppressions
//! incluses, et ne contient que des hachés Argon2id.

use crate::{
    node_settings,
    node_view::{AccountCopy, AccountSnapshot, VerifiedAccount},
    password,
};
use std::{
    path::{Path, PathBuf},
    sync::RwLock,
};

const ACCOUNTS_FILE: &str = "accounts.json";

pub struct AccountStore {
    path: PathBuf,
    accounts: RwLock<Vec<AccountCopy>>,
}

impl AccountStore {
    /// Charge la copie conservée dans `/config` ; un fichier absent ou illisible donne une copie vide.
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join(ACCOUNTS_FILE);
        let accounts = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<AccountSnapshot>(&raw).ok())
            .map(|snapshot| snapshot.accounts)
            .unwrap_or_default();
        Self {
            path,
            accounts: RwLock::new(accounts),
        }
    }

    /// Remplace la copie (en mémoire, puis dans `/config`) et renvoie les comptes actifs.
    pub fn replace(&self, snapshot: AccountSnapshot) -> anyhow::Result<Vec<String>> {
        let active = snapshot
            .accounts
            .iter()
            .filter(|account| !account.disabled)
            .map(|account| account.id.clone())
            .collect();
        node_settings::write_private(&self.path, &serde_json::to_string(&snapshot)?)?;
        *self
            .accounts
            .write()
            .map_err(|_| anyhow::anyhow!("copie des comptes verrouillée"))? = snapshot.accounts;
        Ok(active)
    }

    fn find_active(&self, username: &str) -> Option<AccountCopy> {
        self.accounts
            .read()
            .ok()?
            .iter()
            .find(|account| !account.disabled && account.username.eq_ignore_ascii_case(username))
            .cloned()
    }

    /// Vérifie un mot de passe contre la copie locale ; coût identique si le compte n'existe pas.
    pub async fn verify(&self, username: &str, candidate: String) -> Option<VerifiedAccount> {
        let Some(account) = self.find_active(username) else {
            password::verify_decoy(candidate).await;
            return None;
        };
        password::verify(candidate, account.password_hash)
            .await
            .then_some(VerifiedAccount {
                id: account.id,
                username: account.username,
                role: account.role,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::Role;

    const PASSWORD: &str = "un-bon-mot-de-passe";

    async fn copy(id: &str, username: &str, disabled: bool) -> anyhow::Result<AccountCopy> {
        Ok(AccountCopy {
            id: id.into(),
            username: username.into(),
            password_hash: password::hash(PASSWORD.into())
                .await
                .map_err(|_| anyhow::anyhow!("hachage"))?,
            role: Role::Admin,
            disabled,
            version: 1,
        })
    }

    #[tokio::test]
    async fn test_local_copy_verifies_active_accounts_only() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = AccountStore::load(dir.path());
        let active = store.replace(AccountSnapshot {
            accounts: vec![
                copy("1", "Admin", false).await?,
                copy("2", "ancien", true).await?,
            ],
        })?;
        assert_eq!(active, vec!["1".to_owned()]);
        let ok = store.verify("admin", PASSWORD.into()).await;
        assert_eq!(ok.map(|account| account.id), Some("1".to_owned()));
        assert!(
            store
                .verify("admin", "faux-mot-de-passe".into())
                .await
                .is_none()
        );
        assert!(store.verify("ancien", PASSWORD.into()).await.is_none());
        assert!(store.verify("inconnu", PASSWORD.into()).await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_replacing_drops_deleted_accounts_and_survives_a_restart() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = AccountStore::load(dir.path());
        store.replace(AccountSnapshot {
            accounts: vec![copy("1", "admin", false).await?],
        })?;
        let reloaded = AccountStore::load(dir.path());
        assert!(reloaded.verify("admin", PASSWORD.into()).await.is_some());
        reloaded.replace(AccountSnapshot::default())?;
        assert!(reloaded.verify("admin", PASSWORD.into()).await.is_none());
        assert!(
            AccountStore::load(dir.path())
                .verify("admin", PASSWORD.into())
                .await
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn test_missing_or_corrupted_file_gives_an_empty_copy() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join(ACCOUNTS_FILE), "pas du json")?;
        let store = AccountStore::load(dir.path());
        assert!(store.find_active("admin").is_none());
        Ok(())
    }
}
