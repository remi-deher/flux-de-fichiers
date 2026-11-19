//! Historique des conversations de l'assistant : par compte, chiffré au repos, borné (ADR 0015).
//!
//! Seuls le texte de l'utilisateur et celui de l'assistant sont conservés, jamais un résultat
//! d'outil. Titre et messages sont chiffrés par `SecretBox`, avec le propriétaire dans les données
//! associées : une ligne copiée vers un autre compte ne se déchiffre pas.

use crate::{
    error::{AppError, AppResult},
    secrets::SecretBox,
};
use forgearr_shared::ai::{
    AiChatMessage, AiChatRole, AiConversation, AiConversationList, AiConversationSummary,
};
use sqlx::{Sqlite, SqlitePool, Transaction};

/// Durée de conservation d'une conversation depuis son dernier échange.
pub const HISTORY_RETENTION_DAYS: i64 = 30;
/// Conversations conservées par compte ; les plus anciennes sont supprimées.
pub const MAX_CONVERSATIONS_PER_OWNER: i64 = 50;
const TITLE_MAX_CHARS: usize = 60;
const UNTITLED: &str = "Conversation";

fn aad(owner: &str, field: &str) -> String {
    format!("ai:history:{field}:{owner}")
}

fn role_label(role: AiChatRole) -> &'static str {
    match role {
        AiChatRole::User => "user",
        AiChatRole::Assistant => "assistant",
    }
}

fn parse_role(raw: &str) -> Option<AiChatRole> {
    match raw {
        "user" => Some(AiChatRole::User),
        "assistant" => Some(AiChatRole::Assistant),
        _ => None,
    }
}

/// Titre : première ligne non vide de la question, bornée.
fn title_from(question: &str) -> String {
    let line = question
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(UNTITLED);
    let mut chars = line.chars();
    let head: String = chars.by_ref().take(TITLE_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// Supprime les conversations expirées de `owner`, puis celles au-delà du plafond.
async fn prune(pool: &SqlitePool, owner: &str) -> AppResult<()> {
    let expiry = format!("-{HISTORY_RETENTION_DAYS} days");
    let doomed: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM ai_conversations WHERE owner_id = ?1 AND (
             updated_at < datetime('now', ?2)
             OR id NOT IN (
                 SELECT id FROM ai_conversations WHERE owner_id = ?1
                 ORDER BY updated_at DESC, rowid DESC LIMIT ?3
             )
         )",
    )
    .bind(owner)
    .bind(expiry)
    .bind(MAX_CONVERSATIONS_PER_OWNER)
    .fetch_all(pool)
    .await?;
    for id in doomed {
        delete_rows(pool, owner, &id).await?;
    }
    Ok(())
}

async fn delete_rows(pool: &SqlitePool, owner: &str, id: &str) -> AppResult<bool> {
    let mut tx = pool.begin().await?;
    let owned: Option<String> =
        sqlx::query_scalar("SELECT id FROM ai_conversations WHERE id = ?1 AND owner_id = ?2")
            .bind(id)
            .bind(owner)
            .fetch_optional(&mut *tx)
            .await?;
    if owned.is_none() {
        return Ok(false);
    }
    sqlx::query("DELETE FROM ai_messages WHERE conversation_id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM ai_conversations WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// Enregistre un échange dans `conversation_id` (créée si elle n'existe pas ou n'est pas à `owner`).
/// Renvoie l'identifiant de la conversation.
pub async fn append_exchange(
    pool: &SqlitePool,
    secrets: &SecretBox,
    owner: &str,
    conversation_id: Option<&str>,
    question: &str,
    reply: &str,
) -> AppResult<String> {
    let existing: Option<String> = match conversation_id {
        Some(id) => {
            sqlx::query_scalar("SELECT id FROM ai_conversations WHERE id = ?1 AND owner_id = ?2")
                .bind(id)
                .bind(owner)
                .fetch_optional(pool)
                .await?
        }
        None => None,
    };
    let mut tx = pool.begin().await?;
    let id = match existing {
        Some(id) => {
            sqlx::query("UPDATE ai_conversations SET updated_at = datetime('now') WHERE id = ?1")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            let title = secrets.encrypt(&title_from(question), &aad(owner, "title"))?;
            sqlx::query(
                "INSERT INTO ai_conversations (id, owner_id, title_enc) VALUES (?1, ?2, ?3)",
            )
            .bind(&id)
            .bind(owner)
            .bind(title)
            .execute(&mut *tx)
            .await?;
            id
        }
    };
    for (role, text) in [(AiChatRole::User, question), (AiChatRole::Assistant, reply)] {
        sqlx::query(
            "INSERT INTO ai_messages (conversation_id, role, content_enc) VALUES (?1, ?2, ?3)",
        )
        .bind(&id)
        .bind(role_label(role))
        .bind(secrets.encrypt(text, &aad(owner, "message"))?)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    prune(pool, owner).await?;
    Ok(id)
}

/// Conversations de `owner`, la plus récente d'abord.
pub async fn list(
    pool: &SqlitePool,
    secrets: &SecretBox,
    owner: &str,
) -> AppResult<AiConversationList> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, title_enc, updated_at FROM ai_conversations
         WHERE owner_id = ?1 ORDER BY updated_at DESC, rowid DESC",
    )
    .bind(owner)
    .fetch_all(pool)
    .await?;
    let conversations = rows
        .into_iter()
        .map(|(id, title_enc, updated_at)| {
            let title = secrets
                .decrypt(&title_enc, &aad(owner, "title"))
                .unwrap_or_else(|_| UNTITLED.to_owned());
            AiConversationSummary {
                id,
                title,
                updated_at,
            }
        })
        .collect();
    Ok(AiConversationList { conversations })
}

/// Conversation `id` de `owner` avec ses messages ; `NotFound` si elle n'est pas à lui.
pub async fn load(
    pool: &SqlitePool,
    secrets: &SecretBox,
    owner: &str,
    id: &str,
) -> AppResult<AiConversation> {
    let title_enc: String = sqlx::query_scalar(
        "SELECT title_enc FROM ai_conversations WHERE id = ?1 AND owner_id = ?2",
    )
    .bind(id)
    .bind(owner)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content_enc FROM ai_messages WHERE conversation_id = ?1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let messages = rows
        .into_iter()
        .filter_map(|(role, content_enc)| {
            Some(AiChatMessage {
                role: parse_role(&role)?,
                content: secrets.decrypt(&content_enc, &aad(owner, "message")).ok()?,
            })
        })
        .collect();
    Ok(AiConversation {
        id: id.to_owned(),
        title: secrets
            .decrypt(&title_enc, &aad(owner, "title"))
            .unwrap_or_else(|_| UNTITLED.to_owned()),
        messages,
    })
}

/// Supprime une conversation de `owner` ; `NotFound` si elle n'est pas à lui.
pub async fn delete(pool: &SqlitePool, owner: &str, id: &str) -> AppResult<()> {
    if delete_rows(pool, owner, id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// Supprime toutes les conversations de `owner` ; renvoie leur nombre.
pub async fn delete_all(pool: &SqlitePool, owner: &str) -> AppResult<usize> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM ai_conversations WHERE owner_id = ?1")
            .bind(owner)
            .fetch_all(pool)
            .await?;
    let mut removed = 0;
    for id in ids {
        removed += usize::from(delete_rows(pool, owner, &id).await?);
    }
    Ok(removed)
}

/// Re-chiffre titres et messages lors d'une rotation de clé locale ; renvoie le nombre de valeurs.
pub async fn rotate_secrets_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    old: &SecretBox,
    new: &SecretBox,
) -> AppResult<usize> {
    let mut rotated = 0;
    let titles: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, owner_id, title_enc FROM ai_conversations")
            .fetch_all(&mut **tx)
            .await?;
    for (id, owner, stored) in titles {
        let clear = old.decrypt(&stored, &aad(&owner, "title"))?;
        sqlx::query("UPDATE ai_conversations SET title_enc = ?1 WHERE id = ?2")
            .bind(new.encrypt(&clear, &aad(&owner, "title"))?)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        rotated += 1;
    }
    let messages: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT m.id, c.owner_id, m.content_enc FROM ai_messages m
         JOIN ai_conversations c ON c.id = m.conversation_id",
    )
    .fetch_all(&mut **tx)
    .await?;
    for (id, owner, stored) in messages {
        let clear = old.decrypt(&stored, &aad(&owner, "message"))?;
        sqlx::query("UPDATE ai_messages SET content_enc = ?1 WHERE id = ?2")
            .bind(new.encrypt(&clear, &aad(&owner, "message"))?)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        rotated += 1;
    }
    Ok(rotated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::settings::tests_support::test_pool;

    fn secrets() -> anyhow::Result<SecretBox> {
        Ok(SecretBox::from_key_hex(&SecretBox::generate_key_hex())?)
    }

    #[test]
    fn titles_take_the_first_line_and_are_bounded() {
        assert_eq!(title_from("\n  Bonjour  \nsuite"), "Bonjour");
        assert_eq!(title_from("   "), UNTITLED);
        let long = title_from(&"é".repeat(TITLE_MAX_CHARS + 5));
        assert_eq!(long.chars().count(), TITLE_MAX_CHARS + 1);
        assert!(long.ends_with('…'));
    }

    #[tokio::test]
    async fn exchanges_are_stored_encrypted_continued_and_private_to_their_owner()
    -> anyhow::Result<()> {
        let pool = test_pool().await?;
        let secrets = secrets()?;
        let id = append_exchange(
            &pool,
            &secrets,
            "alice",
            None,
            "Pourquoi ça échoue ?",
            "Parce que.",
        )
        .await?;
        let same = append_exchange(
            &pool,
            &secrets,
            "alice",
            Some(&id),
            "Et ensuite ?",
            "Relancez.",
        )
        .await?;
        assert_eq!(same, id);

        let loaded = load(&pool, &secrets, "alice", &id).await?;
        assert_eq!(loaded.title, "Pourquoi ça échoue ?");
        let texts: Vec<&str> = loaded.messages.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Pourquoi ça échoue ?",
                "Parce que.",
                "Et ensuite ?",
                "Relancez."
            ]
        );

        // Rien de lisible en base.
        let raw: Vec<String> = sqlx::query_scalar("SELECT content_enc FROM ai_messages")
            .fetch_all(&pool)
            .await?;
        assert!(raw.iter().all(|value| !value.contains("Parce que")));

        // Un autre compte ne voit ni ne continue la conversation.
        assert!(list(&pool, &secrets, "bob").await?.conversations.is_empty());
        assert!(matches!(
            load(&pool, &secrets, "bob", &id).await,
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            delete(&pool, "bob", &id).await,
            Err(AppError::NotFound)
        ));
        let other = append_exchange(&pool, &secrets, "bob", Some(&id), "Salut", "Bonjour").await?;
        assert_ne!(other, id);
        assert_eq!(load(&pool, &secrets, "alice", &id).await?.messages.len(), 4);

        delete(&pool, "alice", &id).await?;
        assert!(
            list(&pool, &secrets, "alice")
                .await?
                .conversations
                .is_empty()
        );
        let orphans: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_messages WHERE conversation_id = ?1")
                .bind(&id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(orphans, 0);
        Ok(())
    }

    #[tokio::test]
    async fn old_and_surplus_conversations_are_pruned() -> anyhow::Result<()> {
        let pool = test_pool().await?;
        let secrets = secrets()?;
        let stale = append_exchange(&pool, &secrets, "alice", None, "ancienne", "x").await?;
        sqlx::query(
            "UPDATE ai_conversations SET updated_at = datetime('now', '-31 days') WHERE id = ?1",
        )
        .bind(&stale)
        .execute(&pool)
        .await?;
        for n in 0..MAX_CONVERSATIONS_PER_OWNER {
            append_exchange(&pool, &secrets, "alice", None, &format!("q{n}"), "r").await?;
        }
        let kept = list(&pool, &secrets, "alice").await?.conversations;
        assert_eq!(kept.len(), usize::try_from(MAX_CONVERSATIONS_PER_OWNER)?);
        assert!(kept.iter().all(|c| c.id != stale));
        assert_eq!(
            kept[0].title,
            format!("q{}", MAX_CONVERSATIONS_PER_OWNER - 1)
        );
        // Le plafond est par compte.
        append_exchange(&pool, &secrets, "bob", None, "à moi", "ok").await?;
        assert_eq!(list(&pool, &secrets, "bob").await?.conversations.len(), 1);
        assert_eq!(
            delete_all(&pool, "alice").await?,
            usize::try_from(MAX_CONVERSATIONS_PER_OWNER)?
        );
        assert_eq!(list(&pool, &secrets, "bob").await?.conversations.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn key_rotation_reencrypts_titles_and_messages() -> anyhow::Result<()> {
        let pool = test_pool().await?;
        let old = secrets()?;
        let new = secrets()?;
        let id = append_exchange(&pool, &old, "alice", None, "Titre secret", "Réponse").await?;
        let mut tx = pool.begin().await?;
        assert_eq!(rotate_secrets_in_tx(&mut tx, &old, &new).await?, 3);
        tx.commit().await?;
        let loaded = load(&pool, &new, "alice", &id).await?;
        assert_eq!(loaded.title, "Titre secret");
        assert_eq!(loaded.messages.len(), 2);
        assert!(load(&pool, &old, "alice", &id).await?.messages.is_empty());
        Ok(())
    }
}
