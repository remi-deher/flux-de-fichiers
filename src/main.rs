use flux_de_fichiers::node;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "var/log".into());
    let appender = tracing_appender::rolling::daily(log_dir, "flux.jsonl");
    let (writer, _guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    node::run(std::env::args().nth(1).as_deref()).await
}
