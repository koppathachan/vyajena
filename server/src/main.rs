//! The Pi server binary:
//!
//! - `server --config <file.toml>` runs the server until `SIGTERM` or Ctrl-C.
//! - `server backup --config <file.toml> --to <file>` writes a consistent
//!   snapshot of the server's database to a new file (mode 0600) while the
//!   server keeps running (pi-deployment: Backup to the Mac).

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use server::runtime::{Config, Pi, SystemClock};
use server::store::backup;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let command = command_from_args()?;
    // Config errors stop us here, before any key, database or port is touched
    // (pi-runtime: Configuration).
    let config = Config::load(command.config())?;
    let config = match command {
        Command::Serve { .. } => config,
        Command::Backup { to, .. } => {
            let snapshot = backup::snapshot(&config.database_path, &to)
                .with_context(|| format!("backing up to {}", to.display()))?;
            tracing::info!(
                path = %snapshot.path.display(),
                bytes = snapshot.bytes,
                schema_version = snapshot.schema_version,
                "database snapshot written"
            );
            return Ok(());
        }
    };

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

const USAGE: &str =
    "usage: server --config <file.toml>\n       server backup --config <file.toml> --to <file>";

/// What the binary was asked to do.
enum Command {
    /// Run the server.
    Serve { config: PathBuf },
    /// Write a database snapshot to `to`.
    Backup { config: PathBuf, to: PathBuf },
}

impl Command {
    fn config(&self) -> &std::path::Path {
        match self {
            Self::Serve { config } | Self::Backup { config, .. } => config,
        }
    }
}

fn command_from_args() -> Result<Command> {
    let mut args = std::env::args_os().skip(1).peekable();
    let backup = args.peek().and_then(|a| a.to_str()) == Some("backup");
    if backup {
        args.next();
    }
    let mut config = None;
    let mut to = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config") => config = Some(path_after(&mut args, "--config")?),
            Some("--to") if backup => to = Some(path_after(&mut args, "--to")?),
            Some("--help" | "-h") => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ => bail!("unknown argument {arg:?}\n{USAGE}"),
        }
    }
    let config = config.context("missing --config <file.toml>")?;
    Ok(if backup {
        Command::Backup {
            config,
            to: to.context("missing --to <file>")?,
        }
    } else {
        Command::Serve { config }
    })
}

fn path_after(args: &mut impl Iterator<Item = std::ffi::OsString>, flag: &str) -> Result<PathBuf> {
    match args.next() {
        Some(value) => Ok(PathBuf::from(value)),
        None => bail!("{flag} needs a file path"),
    }
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
