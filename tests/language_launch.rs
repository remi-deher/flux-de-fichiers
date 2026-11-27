//! Lancement confiné d'un serveur de langage par `forgearr sandbox-exec` (ADR 0032 point 4, ADR 0034).
//!
//! Le « serveur » est un petit script qui dit ce qu'il a pu faire. Sans Landlock sur la machine, les essais
//! le disent et s'arrêtent.

#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use forgearr::{confine::detect, language::launch::launch};
use forgearr_shared::language::LanguageServer;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};
use tokio::{io::AsyncBufReadExt, time::timeout};
use uuid::Uuid;

const FORGEARR: &str = env!("CARGO_BIN_EXE_forgearr");
const WAIT: Duration = Duration::from_secs(20);
const EXECUTABLE_MODE: u32 = 0o755;
const TOOL_INTRUDER_FILE: &str = "intrus";
const WORKSPACE_NOTE_FILE: &str = "note";
/// Ce que le serveur d'essai tente, puis annonce ligne à ligne.
const PROBE_SCRIPT: &str = r#"#!/bin/sh
PATH=/usr/bin:/bin
touch "${0%/*}/intrus" 2>/dev/null && echo tool-writable || echo tool-readonly
touch "$HOME/note" 2>/dev/null && echo workspace-writable || echo workspace-readonly
echo "$PWD"
"#;

fn landlock_available() -> bool {
    let available = detect().filters_files();
    if !available {
        eprintln!("Landlock est absent de cette machine : essai ignoré");
    }
    available
}

/// Dossier d'application dont le serveur Python est le script d'essai.
fn tool_with_probe() -> anyhow::Result<tempfile::TempDir> {
    let tool = tempfile::tempdir()?;
    let bin = tool.path().join("bin");
    fs::create_dir_all(&bin)?;
    let server = bin.join(LanguageServer::Python.binary());
    fs::write(&server, PROBE_SCRIPT)?;
    fs::set_permissions(&server, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
    Ok(tool)
}

async fn line_from(reader: &mut (impl AsyncBufReadExt + Unpin)) -> anyhow::Result<String> {
    let mut line = String::new();
    timeout(WAIT, reader.read_line(&mut line)).await??;
    Ok(line.trim().to_owned())
}

#[tokio::test]
async fn the_server_reads_its_tool_and_writes_only_its_workspace() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let tool = tool_with_probe()?;
    let data = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let launcher = Path::new(FORGEARR);
    let server = LanguageServer::Python;
    let mut launched = launch(launcher, tool.path(), data.path(), server, session)?;
    assert_eq!(line_from(&mut launched.reader).await?, "tool-readonly");
    assert_eq!(line_from(&mut launched.reader).await?, "workspace-writable");
    let working_directory = line_from(&mut launched.reader).await?;
    assert_eq!(Path::new(&working_directory), launched.workspace.path());
    assert!(
        launched
            .workspace
            .path()
            .join(WORKSPACE_NOTE_FILE)
            .is_file()
    );
    assert!(!tool.path().join("bin").join(TOOL_INTRUDER_FILE).exists());
    Ok(())
}

#[tokio::test]
async fn the_workspace_goes_with_the_server() -> anyhow::Result<()> {
    if !landlock_available() {
        return Ok(());
    }
    let tool = tool_with_probe()?;
    let data = tempfile::tempdir()?;
    let launcher = Path::new(FORGEARR);
    let server = LanguageServer::Python;
    let launched = launch(launcher, tool.path(), data.path(), server, Uuid::new_v4())?;
    let workspace = launched.workspace.path().to_path_buf();
    assert!(workspace.is_dir());
    drop(launched);
    assert!(!workspace.exists());
    Ok(())
}
