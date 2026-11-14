use flux_de_fichiers::config::central_url;

#[test]
fn remote_central_requires_no_embedded_credentials() {
    for invalid in [
        "ftp://localhost",
        "https://user:password@central",
        "https://central/path",
        "https://central/?token=secret",
        "https://central/#fragment",
        "http://user:password@central",
        "http://central/path",
    ] {
        assert!(central_url(invalid).is_err(), "{invalid}");
    }
    for valid in [
        "http://central:3000",
        "https://central:3443",
        "http://127.0.0.1:3000",
        "http://localhost:3000",
    ] {
        assert!(central_url(valid).is_ok(), "{valid}");
    }
}

#[test]
fn admin_and_worker_roles_are_distinct() {
    use flux_shared::{Permission, Role};
    assert!(Role::Admin.allows(Permission::ManageApiKeys));
    assert!(!Role::Admin.allows(Permission::WorkerProtocol));
    assert!(Role::Worker.allows(Permission::WorkerProtocol));
    assert!(!Role::Worker.allows(Permission::ManageApiKeys));
}
