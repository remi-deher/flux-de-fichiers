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
fn admin_and_worker_roles_are_distinct() {
    use flux_shared::{Permission, Role};
    assert!(Role::Admin.allows(Permission::ManageApiKeys));
    assert!(!Role::Admin.allows(Permission::NodeAgent));
    assert!(Role::Worker.allows(Permission::NodeAgent));
    assert!(!Role::Worker.allows(Permission::ManageApiKeys));
}
