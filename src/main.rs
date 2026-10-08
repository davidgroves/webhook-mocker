use std::net::SocketAddr;

use clap::Parser;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use webhook_mocker::{
    AppState, build_router,
    config::{Cli, Command, Settings},
    store::Store,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let settings = Settings::load(&cli)?;

    init_tracing(settings.json_logs);

    if matches!(cli.command, Some(Command::Healthcheck)) {
        return run_healthcheck(settings.listen).await;
    }

    settings.warn_if_browser_blocked();

    let store = Store::new(settings.max_messages, settings.snapshot.clone());
    for preset in &settings.channels {
        let ch = store.create_channel(
            preset.kind,
            preset.name.clone(),
            preset.token.clone(),
            preset.slack_team.clone(),
            preset.slack_bot.clone(),
            preset.faults.clone(),
        );
        tracing::info!(
            kind = ch.kind.as_str(),
            name = %ch.name,
            url = %ch.webhook_url(&settings.public_base()),
            force_429 = ch.faults.force_429,
            fail_percent = ?ch.faults.fail_percent,
            delay_ms = ?ch.faults.delay_ms,
            "preset channel ready"
        );
    }

    let state = AppState {
        store,
        public_base: settings.public_base(),
        strict: settings.strict,
        auto_create: settings.auto_create,
    };

    let app = build_router(state, settings.max_body);

    let listener = tokio::net::TcpListener::bind(settings.listen).await?;
    tracing::info!(
        listen = %settings.listen,
        public = %settings.public_base(),
        "webhook-mocker listening"
    );
    tracing::info!("UI: {}/", settings.public_base());

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if json {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }
}

async fn run_healthcheck(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    let host = addr.ip();
    let req = format!("GET /healthz HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if response.contains("200") {
        Ok(())
    } else {
        Err(format!("healthcheck failed: {response}").into())
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
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
    tracing::info!("shutdown signal received");
}
