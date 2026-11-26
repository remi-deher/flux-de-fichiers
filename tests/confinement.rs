//! Lancement confiné d'un vrai programme par `forgearr sandbox-exec` (ADR 0034, phase 1).
//!
//! Chaque essai lance le binaire de Forgearr, qui s'applique le profil puis remplace son processus par un
//! programme système. Sans Landlock sur la machine, les essais qui en dépendent le disent et s'arrêtent.

#![cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]

use forgearr::confine::{EXIT_LAUNCH_FAILED, EXIT_SETUP_FAILED, confined_command_with, detect};
use forgearr_shared::confinement::{ConfinementProfile, NetworkAccess, ResourceLimits};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::{Command, Output},
};

const FORGEARR: &str = env!("CARGO_BIN_EXE_forgearr");
const OVERSIZED_WRITE_BYTES: &str = "100000";
const FILE_SIZE_LIMIT_BYTES: u64 = 1000;

fn run(profile: &ConfinementProfile, program: &str, arguments: &[&str]) -> anyhow::Result<Output> {
    let mut command = confined_command_with(Path::new(FORGEARR), profile, program, arguments)?;
    Ok(command.output()?)
}

fn landlock_available() -> bool {
    let available = detect().filters_files();
    if !available {
        eprintln!("Landlock est absent de cette machine : essai ignoré");
    }
    available
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn file_in(directory: &tempfile::TempDir, name: &str, content: &str) -> anyhow::Result<String> {
    let path = directory.path().join(name);
    fs::write(&path, content)?;
    Ok(path.to_string_lossy().into_owned())
}

#[test]
fn a_file_in_a_granted_directory_can_be_read() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let file = file_in(&directory, "lisible.txt", "contenu")?;
    let profile = ConfinementProfile::runtime().reading(directory.path());
    let output = run(&profile, "cat", &[&file])?;
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "contenu");
    Ok(())
}

#[test]
fn a_file_outside_the_granted_directories_cannot_be_read() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let granted = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let secret = file_in(&other, "secret.txt", "secret")?;
    let profile = ConfinementProfile::runtime().reading(granted.path());
    let output = run(&profile, "cat", &[&secret])?;
    assert!(!output.status.success());
    assert!(stderr_of(&output).contains("Permission denied"));
    assert!(output.stdout.is_empty());
    Ok(())
}

#[test]
fn a_writable_directory_accepts_new_files() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("sortie.txt");
    let command = format!("echo ecrit > {}", target.display());
    let profile = ConfinementProfile::runtime().writing(directory.path());
    let output = run(&profile, "sh", &["-c", &command])?;
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(fs::read_to_string(target)?, "ecrit\n");
    Ok(())
}

#[test]
fn a_read_only_directory_refuses_new_files() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("intrus.txt");
    let command = format!("echo intrus > {}", target.display());
    let profile = ConfinementProfile::runtime().reading(directory.path());
    let output = run(&profile, "sh", &["-c", &command])?;
    assert!(!output.status.success());
    assert!(!target.exists());
    Ok(())
}

#[test]
fn the_network_is_closed_unless_the_profile_opens_it() -> anyhow::Result<()> {
    if !detect().filters_network() {
        eprintln!("Landlock ne filtre pas le réseau sur cette machine : essai ignoré");
        return Ok(());
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let connect = format!("exec 3<>/dev/tcp/127.0.0.1/{}", listener.local_addr()?.port());
    let closed = ConfinementProfile::runtime();
    assert!(!run(&closed, "bash", &["-c", &connect])?.status.success());
    let open = ConfinementProfile::runtime().with_network(NetworkAccess::Any);
    let output = run(&open, "bash", &["-c", &connect])?;
    assert!(output.status.success(), "{}", stderr_of(&output));
    Ok(())
}

#[test]
fn a_file_larger_than_the_limit_cannot_be_written() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("gros.bin");
    let command = format!("head -c {OVERSIZED_WRITE_BYTES} /dev/zero > {}", target.display());
    let limits = ResourceLimits {
        max_file_bytes: Some(FILE_SIZE_LIMIT_BYTES),
        ..ResourceLimits::default()
    };
    let profile = ConfinementProfile::runtime()
        .writing(directory.path())
        .with_limits(limits);
    let output = run(&profile, "sh", &["-c", &command])?;
    assert!(!output.status.success());
    assert!(fs::metadata(target)?.len() <= FILE_SIZE_LIMIT_BYTES);
    Ok(())
}

#[test]
fn denied_system_calls_fail_with_a_permission_error() -> anyhow::Result<()> {
    if !landlock_available() || !detect().seccomp {
        return Ok(());
    }
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("python3 est absent de cette machine : essai ignoré");
        return Ok(());
    }
    let ptrace_without_request = format!(
        "import ctypes, sys; libc = ctypes.CDLL(None, use_errno=True); \
         result = libc.syscall({}, 0, 0, 0, 0); \
         sys.exit(0 if result == -1 and ctypes.get_errno() == 1 else 1)",
        libc::SYS_ptrace
    );
    let output = run(&ConfinementProfile::runtime(), "python3", &["-c", &ptrace_without_request])?;
    assert!(output.status.success(), "{}", stderr_of(&output));
    Ok(())
}

#[test]
fn a_missing_program_exits_with_the_launch_code() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let output = run(&ConfinementProfile::runtime(), "/nonexistent/programme", &[])?;
    assert_eq!(output.status.code(), Some(EXIT_LAUNCH_FAILED));
    assert!(stderr_of(&output).contains("lancement impossible"));
    Ok(())
}

#[test]
fn a_bad_invocation_exits_with_the_setup_code_and_the_usage() -> anyhow::Result<()> {
    let output = Command::new(FORGEARR).arg("sandbox-exec").output()?;
    assert_eq!(output.status.code(), Some(EXIT_SETUP_FAILED));
    assert!(stderr_of(&output).contains("usage"));
    Ok(())
}

#[test]
fn an_unreadable_profile_exits_with_the_setup_code() -> anyhow::Result<()> {
    let output = Command::new(FORGEARR)
        .args(["sandbox-exec", "--profile", "pas du json", "--", "true"])
        .output()?;
    assert_eq!(output.status.code(), Some(EXIT_SETUP_FAILED));
    assert!(stderr_of(&output).contains("illisible"));
    Ok(())
}
