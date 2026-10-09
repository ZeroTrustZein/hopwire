use hopwire_server::{app, AppState, ServerConfig};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();
    let cfg = ServerConfig::from_env()?;
    let state = AppState::init(&cfg).await?;

    let (shutdown_tx, _shutdown_rx) = watch::channel(false);

    #[cfg(feature = "kafka")]
    if let Some(kafka_cfg) = hopwire_server::kafka::consumer::KafkaConfig::from_env() {
        let st_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) =
                hopwire_server::kafka::consumer::run_consumer(st_clone, kafka_cfg, _shutdown_rx)
                    .await
            {
                tracing::error!(error = %e, "Kafka consumer terminated with error");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!(bind = %cfg.bind, "hopwire-server listening");
    axum::serve(listener, app(state))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(true);
        })
        .await?;
    Ok(())
}
