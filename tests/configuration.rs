use flux_de_fichiers::config::coordinator_url;

#[test]
fn remote_coordinator_requires_no_embedded_credentials() {
    for invalid in [
        "ftp://localhost",
        "https://user:password@coordinator",
        "https://coordinator/path",
        "https://coordinator/?token=secret",
        "https://coordinator/#fragment",
        "http://user:password@coordinator",
        "http://coordinator/path",
    ] {
        assert!(coordinator_url(invalid).is_err(), "{invalid}");
    }
    for valid in [
        "http://coordinator:3000",
        "https://coordinator:3443",
        "http://127.0.0.1:3000",
        "http://localhost:3000",
    ] {
        assert!(coordinator_url(valid).is_ok(), "{valid}");
    }
}

#[test]
fn admin_and_worker_roles_are_distinct() -> anyhow::Result<()> {
    use axum::http::HeaderMap;
    use flux_de_fichiers::auth::authorize;
    let mut headers = HeaderMap::new();
    assert!(authorize(&headers, "admin").is_err());
    headers.insert("authorization", "Bearer worker".parse()?);
    assert!(authorize(&headers, "admin").is_err());
    assert!(authorize(&headers, "worker").is_ok());
    Ok(())
}
