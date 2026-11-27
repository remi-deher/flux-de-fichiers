//! Lancement confiné d'un serveur de langage (ADR 0032 point 4, ADR 0034).
//!
//! Le serveur lit le dossier de l'application `langages` et écrit dans un dossier de travail qui lui est
//! propre ; il n'a pas de réseau. Le dossier de travail porte son `HOME` et ses caches, et disparaît avec la
//! session.

use crate::{confine, supervision::SupervisedChild, tools_cache::ToolCacheManager};
use forgearr_shared::{
    confinement::{ConfinementProfile, NetworkAccess},
    language::{LANGUAGE_TOOL_ID, LanguageServer},
};
use std::{
    io,
    path::{Path, PathBuf},
    process::Stdio,
};
use thiserror::Error;
use tokio::{
    io::BufReader,
    process::{ChildStdin, ChildStdout, Command},
};
use uuid::Uuid;

/// Dossier des espaces de travail des sessions, sous le dossier de données.
const WORKSPACES_DIRECTORY: &str = "language";
/// Sous-dossier des binaires dans le dossier d'une application du cache.
const BINARY_DIRECTORY: &str = "bin";
const HOME_VARIABLE: &str = "HOME";
const PATH_VARIABLE: &str = "PATH";
const CACHE_VARIABLE: &str = "XDG_CACHE_HOME";
const TEMPORARY_VARIABLE: &str = "TMPDIR";

#[derive(Debug, Error)]
pub enum LaunchError {
    /// L'application `langages` n'est pas installée, ou ne contient pas ce serveur.
    #[error("serveur de langage « {0} » absent")]
    Missing(&'static str),
    #[error("lancement du serveur de langage impossible : {0}")]
    Failed(String),
}

/// Dossier de travail d'une session, supprimé quand elle se termine.
#[derive(Debug)]
pub struct Workspace(PathBuf);

impl Workspace {
    /// Crée le dossier de la session sous `data_dir`.
    pub fn create(data_dir: &Path, session: Uuid) -> io::Result<Self> {
        let path = data_dir
            .join(WORKSPACES_DIRECTORY)
            .join(session.to_string());
        std::fs::create_dir_all(&path)?;
        Ok(Self(std::fs::canonicalize(path)?))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Serveur lancé : ses tubes, le processus qui meurt avec ce qui le détient, et son dossier de travail.
pub struct LaunchedServer {
    pub reader: BufReader<ChildStdout>,
    pub writer: ChildStdin,
    pub child: SupervisedChild,
    pub workspace: Workspace,
}

/// Dossier de la version installée de l'application `langages`, `None` si elle n'est pas prête.
pub fn installed_tool_dir(cache: &ToolCacheManager, version: &str) -> Option<PathBuf> {
    let arch = crate::tools_cache::host_arch();
    cache
        .is_ready(LANGUAGE_TOOL_ID, version, arch)
        .then(|| cache.tool_dir(LANGUAGE_TOOL_ID, version, arch))
}

/// Profil du serveur : lecture du système et du dossier de l'application, écriture dans `workspace`, pas
/// de réseau.
pub fn server_profile(tool_dir: &Path, workspace: &Path) -> ConfinementProfile {
    ConfinementProfile::runtime()
        .reading(tool_dir)
        .writing(workspace)
        .with_network(NetworkAccess::None)
}

/// Binaire du serveur dans le dossier de l'application ; `None` s'il n'y est pas.
pub fn server_binary(tool_dir: &Path, server: LanguageServer) -> Option<PathBuf> {
    let binary =
        ToolCacheManager::resolve_bin_candidate(&tool_dir.join(BINARY_DIRECTORY), server.binary());
    binary.is_file().then_some(binary)
}

/// Commande du serveur : programme et environnement réduit à ce dont il a besoin.
fn server_command(
    launcher: &Path,
    tool_dir: &Path,
    workspace: &Path,
    server: LanguageServer,
    binary: &Path,
) -> Result<Command, LaunchError> {
    let profile = server_profile(tool_dir, workspace);
    let mut command = Command::from(
        confine::confined_command_with(launcher, &profile, binary, server.arguments())
            .map_err(|error| LaunchError::Failed(error.to_string()))?,
    );
    command
        .current_dir(workspace)
        .env_clear()
        .env(HOME_VARIABLE, workspace)
        .env(CACHE_VARIABLE, workspace)
        .env(TEMPORARY_VARIABLE, workspace)
        .env(PATH_VARIABLE, tool_dir.join(BINARY_DIRECTORY))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Ok(command)
}

/// Lance `server` confiné par `launcher` (l'exécutable de Forgearr), avec un dossier de travail neuf sous
/// `data_dir`.
pub fn launch(
    launcher: &Path,
    tool_dir: &Path,
    data_dir: &Path,
    server: LanguageServer,
    session: Uuid,
) -> Result<LaunchedServer, LaunchError> {
    let binary = server_binary(tool_dir, server).ok_or(LaunchError::Missing(server.binary()))?;
    let workspace = Workspace::create(data_dir, session)
        .map_err(|error| LaunchError::Failed(error.to_string()))?;
    let mut command = server_command(launcher, tool_dir, workspace.path(), server, &binary)?;
    let mut child = SupervisedChild::spawn(&mut command)
        .map_err(|error| LaunchError::Failed(error.to_string()))?;
    let (Some(stdin), Some(stdout)) = (child.take_stdin(), child.take_stdout()) else {
        return Err(LaunchError::Failed("tubes du serveur indisponibles".into()));
    };
    Ok(LaunchedServer {
        reader: BufReader::new(stdout),
        writer: stdin,
        child,
        workspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_with_binary(name: &str) -> anyhow::Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join(BINARY_DIRECTORY))?;
        std::fs::write(dir.path().join(BINARY_DIRECTORY).join(name), b"")?;
        Ok(dir)
    }

    #[test]
    fn the_workspace_exists_while_held_and_goes_with_the_session() -> anyhow::Result<()> {
        let data = tempfile::tempdir()?;
        let workspace = Workspace::create(data.path(), Uuid::new_v4())?;
        let path = workspace.path().to_path_buf();
        assert!(path.is_dir());
        assert!(path.is_absolute());
        drop(workspace);
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn two_sessions_never_share_a_workspace() -> anyhow::Result<()> {
        let data = tempfile::tempdir()?;
        let first = Workspace::create(data.path(), Uuid::new_v4())?;
        let second = Workspace::create(data.path(), Uuid::new_v4())?;
        assert_ne!(first.path(), second.path());
        Ok(())
    }

    #[test]
    fn the_profile_reads_the_tool_writes_the_workspace_and_has_no_network() {
        let tool = PathBuf::from("/cache/langages");
        let workspace = PathBuf::from("/data/language/x");
        let profile = server_profile(&tool, &workspace);
        assert!(profile.read_paths.contains(&tool));
        assert!(profile.write_paths.contains(&workspace));
        assert_eq!(profile.network, NetworkAccess::None);
        assert!(!profile.write_paths.contains(&tool));
    }

    #[test]
    fn a_binary_is_found_in_the_bin_directory() -> anyhow::Result<()> {
        let tool = tool_with_binary("ty")?;
        let found = server_binary(tool.path(), LanguageServer::Python);
        assert_eq!(found, Some(tool.path().join("bin").join("ty")));
        assert_eq!(server_binary(tool.path(), LanguageServer::Bash), None);
        Ok(())
    }

    #[test]
    fn an_application_that_is_not_ready_is_not_installed() -> anyhow::Result<()> {
        let cache_root = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(cache_root.path());
        assert_eq!(installed_tool_dir(&cache, "1.0.0"), None);
        let dir = cache.tool_dir(LANGUAGE_TOOL_ID, "1.0.0", crate::tools_cache::host_arch());
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(".ready"), b"")?;
        assert_eq!(installed_tool_dir(&cache, "1.0.0"), Some(dir));
        Ok(())
    }

    #[test]
    fn the_command_runs_the_sandbox_with_a_reduced_environment() -> anyhow::Result<()> {
        let tool = tool_with_binary("ty")?;
        let workspace = tempfile::tempdir()?;
        let binary = tool.path().join("bin").join("ty");
        let server = LanguageServer::Python;
        let launcher = Path::new("/opt/forgearr");
        let command = server_command(launcher, tool.path(), workspace.path(), server, &binary)?;
        let std_command = command.as_std();
        assert_eq!(std_command.get_program(), launcher.as_os_str());
        assert_eq!(std_command.get_current_dir(), Some(workspace.path()));
        let home = std_command
            .get_envs()
            .find(|(name, _)| *name == HOME_VARIABLE)
            .and_then(|(_, value)| value);
        assert_eq!(home, Some(workspace.path().as_os_str()));
        let arguments: Vec<String> = std_command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(arguments.last().map(String::as_str), Some("server"));
        Ok(())
    }

    #[test]
    fn a_missing_server_is_reported_by_name() -> anyhow::Result<()> {
        let tool = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let server = LanguageServer::Python;
        let launcher = Path::new("/opt/forgearr");
        let result = launch(launcher, tool.path(), data.path(), server, Uuid::new_v4());
        assert!(matches!(result, Err(LaunchError::Missing("ty"))));
        Ok(())
    }
}
