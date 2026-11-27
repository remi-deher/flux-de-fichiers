use super::*;
use crate::{api::transfers::tests::setup, api_keys};
use axum::http::{HeaderValue, Uri, header};
use forgearr_shared::{Role, language::INTERPRETER_QUERY_PARAMETER};

fn bearer(raw: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {raw}"))?,
    );
    Ok(headers)
}

#[test]
fn the_interpreter_is_read_from_the_shared_query_parameter() -> anyhow::Result<()> {
    let query = format!("{INTERPRETER_QUERY_PARAMETER}=python3");
    let uri: Uri = format!("/api/v1/language/session?{query}").parse()?;
    let Query(query) = Query::<SessionQuery>::try_from_uri(&uri)?;
    assert_eq!(query.interpreter, ScriptInterpreter::Python3);
    Ok(())
}

#[test]
fn an_unknown_interpreter_is_refused() -> anyhow::Result<()> {
    let query = format!("{INTERPRETER_QUERY_PARAMETER}=cobol");
    let uri: Uri = format!("/api/v1/language/session?{query}").parse()?;
    assert!(Query::<SessionQuery>::try_from_uri(&uri).is_err());
    Ok(())
}

#[tokio::test]
async fn an_administrator_keeps_the_channel() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let key = api_keys::issue(&state.pool, "flux", Role::Admin).await?;
    assert!(still_allowed(&state, &bearer(&key.raw)?).await);
    Ok(())
}

#[tokio::test]
async fn a_caller_without_the_right_loses_the_channel() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let key = api_keys::issue(&state.pool, "flux", Role::Worker).await?;
    assert!(!still_allowed(&state, &bearer(&key.raw)?).await);
    assert!(!still_allowed(&state, &HeaderMap::new()).await);
    Ok(())
}

#[tokio::test]
async fn a_revoked_key_loses_the_channel() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, _, _, _) = setup(dir.path()).await?;
    let key = api_keys::issue(&state.pool, "flux", Role::Admin).await?;
    let headers = bearer(&key.raw)?;
    assert!(still_allowed(&state, &headers).await);
    crate::repository::revoke_api_key(&state.pool, &key.record.id).await?;
    assert!(!still_allowed(&state, &headers).await);
    Ok(())
}
