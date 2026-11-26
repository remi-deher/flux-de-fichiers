use forgearr::{
    instance,
    logs::{LOG_FILE_PREFIX, log_dir},
};

/// Un lancement confiné (`sandbox-exec`, ADR 0034) s'exécute avant toute initialisation : ni journaux, ni
/// runtime asynchrone, un seul fil.
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn run_confined_launch_if_requested() {
    use forgearr::confine::{SANDBOX_EXEC_COMMAND, run_sandbox_exec};
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some((command, launch_arguments)) = arguments.split_first()
        && command == SANDBOX_EXEC_COMMAND
    {
        std::process::exit(run_sandbox_exec(launch_arguments));
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn run_confined_launch_if_requested() {}

fn main() -> anyhow::Result<()> {
    run_confined_launch_if_requested();
    serve()
}

#[tokio::main]
async fn serve() -> anyhow::Result<()> {
    let appender = tracing_appender::rolling::daily(log_dir(), LOG_FILE_PREFIX);
    let (writer, _guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next();
    let operand = arguments.next();
    instance::run(command.as_deref(), operand.as_deref()).await
}
