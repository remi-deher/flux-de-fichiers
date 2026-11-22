//! Supervision de ce qu'une tâche laisse derrière elle quand elle est abandonnée (ADR 0027) : processus
//! externes et sections critiques. Une tâche n'est rendue au central qu'une fois tout terminé.

use std::{
    future::Future,
    io,
    process::ExitStatus,
    time::{Duration, Instant},
};
use tokio::{
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::mpsc,
};

/// Attente de la disparition du groupe après la mort de son chef. Au-delà, il ne reste que des
/// zombies sans parent pour les recueillir : `SIGKILL` leur a déjà été envoyé.
const GROUP_END_TIMEOUT: Duration = Duration::from_secs(2);
const GROUP_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Preuve qu'un travail de la tâche court encore ; rendue à sa fin.
pub type WorkToken = mpsc::Sender<()>;

tokio::task_local! {
    static CURRENT_WORK: WorkToken;
}

/// Travaux en cours d'une tâche.
pub struct TaskWork {
    token: Option<WorkToken>,
    ended: mpsc::Receiver<()>,
}

impl Default for TaskWork {
    fn default() -> Self {
        let (token, ended) = mpsc::channel(1);
        Self {
            token: Some(token),
            ended,
        }
    }
}

impl TaskWork {
    /// Exécute `future` en comptant les processus qu'elle lance.
    pub fn scope<F: Future>(&self, future: F) -> impl Future<Output = F::Output> + use<F> {
        let token = self.token.clone();
        async move {
            match token {
                Some(token) => CURRENT_WORK.scope(token, future).await,
                None => future.await,
            }
        }
    }

    /// Attend la fin de tous les travaux, une fois la future de [`Self::scope`] abandonnée.
    /// `false` si elle n'est pas constatée avant `limit`.
    pub async fn ended(&mut self, limit: Duration) -> bool {
        self.token = None;
        tokio::time::timeout(limit, self.ended.recv()).await.is_ok()
    }
}

/// Preuve de travail de la tâche courante, à confier à ce qui lui survit.
pub fn current_token() -> Option<WorkToken> {
    CURRENT_WORK.try_with(Clone::clone).ok()
}

/// Mène `work` à son terme même si l'appelant est abandonné (remplacement d'une source…).
pub async fn critical<T, F>(work: F) -> io::Result<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let token = current_token();
    let handle = tokio::spawn(async move {
        let outcome = work.await;
        drop(token);
        outcome
    });
    handle.await.map_err(io::Error::other)
}

/// Processus externe lancé dans son propre groupe : abandonné, tout le groupe est tué.
pub struct SupervisedChild {
    child: Option<Child>,
    group: Option<u32>,
    token: Option<WorkToken>,
}

impl SupervisedChild {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        command.kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn()?;
        Ok(Self {
            group: child.id(),
            child: Some(child),
            token: current_token(),
        })
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        match self.child.as_mut() {
            Some(child) => child.wait().await,
            None => Err(io::Error::other("processus déjà rendu")),
        }
    }

    /// Fin normale : plus rien à surveiller ni à tuer.
    pub fn finished(mut self) {
        self.child = None;
        self.group = None;
    }
}

impl Drop for SupervisedChild {
    fn drop(&mut self) {
        let Some(group) = self.group.take() else {
            return;
        };
        signal_group(group, kill_signal());
        let (Some(mut child), Ok(runtime)) =
            (self.child.take(), tokio::runtime::Handle::try_current())
        else {
            return;
        };
        let token = self.token.take();
        runtime.spawn(async move {
            let _ = child.kill().await;
            wait_group_end(group).await;
            drop(token);
        });
    }
}

async fn wait_group_end(group: u32) {
    let deadline = Instant::now() + GROUP_END_TIMEOUT;
    while signal_group(group, 0) && Instant::now() < deadline {
        tokio::time::sleep(GROUP_POLL_INTERVAL).await;
    }
}

#[cfg(unix)]
const fn kill_signal() -> i32 {
    libc::SIGKILL
}

#[cfg(not(unix))]
const fn kill_signal() -> i32 {
    0
}

/// Envoie un signal au groupe ; `false` s'il n'existe plus.
#[cfg(unix)]
fn signal_group(group: u32, signal: i32) -> bool {
    let Ok(group) = libc::pid_t::try_from(group) else {
        return false;
    };
    // SAFETY: `killpg` ne lit aucune mémoire ; le groupe a été créé par `process_group(0)` au
    // lancement du processus surveillé, dont l'identifiant est connu.
    unsafe { libc::killpg(group, signal) == 0 }
}

#[cfg(not(unix))]
fn signal_group(_group: u32, _signal: i32) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Stdio;

    const LIMIT: Duration = Duration::from_secs(10);

    #[tokio::test]
    async fn an_abandoned_task_kills_its_whole_process_group() -> anyhow::Result<()> {
        let mut work = TaskWork::default();
        let (started, group) = tokio::sync::oneshot::channel();
        let abandoned = work.scope(async move {
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", "sleep 600 & wait"])
                .stdout(Stdio::null());
            let mut child = SupervisedChild::spawn(&mut command)?;
            let _ = started.send(child.group);
            child.wait().await
        });
        let mut running = Box::pin(abandoned);
        let group = tokio::select! {
            group = group => group?,
            _ = &mut running => None,
        };
        let group = group.ok_or_else(|| anyhow::anyhow!("processus non lancé"))?;
        assert!(signal_group(group, 0));
        drop(running);
        assert!(work.ended(LIMIT).await);
        assert!(!std::path::Path::new(&format!("/proc/{group}")).exists());
        Ok(())
    }

    #[tokio::test]
    async fn a_finished_process_leaves_nothing_to_wait_for() -> anyhow::Result<()> {
        let mut work = TaskWork::default();
        let status = work
            .scope(async {
                let mut child = SupervisedChild::spawn(&mut Command::new("/bin/true"))?;
                let status = child.wait().await;
                child.finished();
                status
            })
            .await?;
        assert!(status.success());
        assert!(work.ended(LIMIT).await);
        Ok(())
    }

    #[tokio::test]
    async fn a_critical_section_outlives_its_caller() -> anyhow::Result<()> {
        let mut work = TaskWork::default();
        let (release, gate) = tokio::sync::oneshot::channel::<()>();
        let (done, finished) = tokio::sync::oneshot::channel();
        let section = critical(async move {
            let _ = gate.await;
            let _ = done.send(());
        });
        let mut running = Box::pin(work.scope(section));
        tokio::select! {
            _ = &mut running => anyhow::bail!("section terminée trop tôt"),
            () = tokio::task::yield_now() => {}
        }
        drop(running);
        assert!(!work.ended(Duration::from_millis(20)).await);
        let _ = release.send(());
        finished.await?;
        assert!(work.ended(LIMIT).await);
        Ok(())
    }
}
