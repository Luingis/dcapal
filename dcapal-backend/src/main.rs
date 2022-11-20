use std::{fs::File, net::SocketAddr};

use dcapal_backend::{config::Config, DcaServer};
use futures::FutureExt;
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::signal;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::new()?;

    let _guard = init_tracing(&config.app.log)?;
    init_prometheus_exporter(&config.server.metrics)?;

    let mut server = DcaServer::try_new(config)?;

    Ok(server.start(shutdown_signal().boxed()).await?)
}

fn init_tracing(config: &dcapal_backend::config::Log) -> anyhow::Result<Option<WorkerGuard>> {
    let mut layers = vec![];

    let mut file_guard = None;
    if let Some(ref file_path) = config.file {
        let file = File::create(file_path)?;

        let (non_blocking, guard) = tracing_appender::non_blocking(file);
        file_guard.replace(guard);

        let layer = tracing_subscriber::fmt::layer()
            .with_thread_names(true)
            .with_target(true)
            .with_writer(non_blocking)
            .json()
            .boxed();

        layers.push(layer);
    }

    if config.enable_stdout || config.file.is_none() {
        layers.push(tracing_subscriber::fmt::layer().boxed());
    }

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| config.level.clone()),
        ))
        .with(layers)
        .init();

    Ok(file_guard)
}

fn init_prometheus_exporter(config: &dcapal_backend::config::Metrics) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.hostname, config.port).parse()?;

    PrometheusBuilder::new()
        .add_global_label("job", "dcapal-backend")
        .with_http_listener(addr)
        .install()?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("Signal received, starting graceful shutdown");
}
