//! The Pi server binary: `server --config <file.toml>`.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use server::runtime::{Config, Pi, SystemClock};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config_path = config_path_from_args()?;
    // Config errors stop us here, before any key, database or port is touched
    // (pi-runtime: Configuration).
    let config = Config::load(&config_path)?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?
        .block_on(async {
            let pi = Pi::start(config, Arc::new(SystemClock)).await?;
            pi.serve(shutdown_signal()).await?;
            Ok(())
        })
}

fn config_path_from_args() -> Result<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    let mut path = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config") => match args.next() {
                Some(value) => path = Some(PathBuf::from(value)),
                None => bail!("--config needs a file path"),
            },
            Some("--help" | "-h") => {
                println!("usage: server --config <file.toml>");
                std::process::exit(0);
            }
            _ => bail!("unknown argument {arg:?}; usage: server --config <file.toml>"),
        }
    }
    path.context("missing --config <file.toml>")
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(term) => term,
                Err(_) => {
                    let _ = ctrl_c.await;
                    return;
                }
            };
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
    tracing::info!("shutting down");
}
