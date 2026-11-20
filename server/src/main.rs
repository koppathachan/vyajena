//! The server binary:
//!
//! - `server --config <file.toml> [--push <push.toml>]` runs the server until
//!   `SIGTERM` or Ctrl-C; with `--push`, wake pings go to FCM / APNs
//!   (server-runtime: Push provider interface), else they are only logged.
//! - `server egress-proxy --listen <addr>` runs the CONNECT proxy to the push
//!   hosts (server-runtime: Egress proxy).
//! - `server backup --config <file.toml> --to <file>` writes a consistent
//!   snapshot of the server's database to a new file (mode 0600) while the
//!   Server keeps running (server-deployment: Backup to the Mac).

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use server::push::{LoggingProvider, Options, Providers, PushProvider, proxy};
use server::runtime::{Config, Server, SystemClock};
use server::store::backup;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .with_env_filter(server::runtime::logging::filter())
        .init();

    // Config errors stop us here, before any key, database or port is touched
    // (server-runtime: Configuration).
    let (config, push) = match command_from_args()? {
        Command::EgressProxy { listen } => return egress_proxy(listen),
        Command::Backup { config, to } => {
            let config = Config::load(&config)?;
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
        Command::Serve { config, push } => (Config::load(&config)?, push),
    };

    // Push errors stop us here too, before any key, database or port
    // (server-runtime: Push configuration).
    let push: Arc<dyn PushProvider> = match push {
        None => {
            tracing::info!("no --push: wake pings are only logged");
            Arc::new(LoggingProvider)
        }
        Some(path) => {
            let providers = Providers::load(&path, &Options::default())
                .with_context(|| format!("push config {}", path.display()))?;
            tracing::info!(
                fcm = providers.has_fcm(),
                apns = providers.has_apns(),
                "push providers (a platform without credentials only logs its pings)"
            );
            Arc::new(providers)
        }
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?
        .block_on(async {
            let server = Server::start_with_push(config, Arc::new(SystemClock), push).await?;
            server.serve(shutdown_signal()).await?;
            Ok(())
        })
}

const USAGE: &str = "usage: Server --config <file.toml> [--push <push.toml>]
       server backup --config <file.toml> --to <file>
       server egress-proxy --listen <addr>";

/// server-runtime: Egress proxy, until `SIGTERM` or Ctrl-C.
fn egress_proxy(listen: std::net::SocketAddr) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?
        .block_on(async {
            let listener = tokio::net::TcpListener::bind(listen)
                .await
                .with_context(|| format!("binding {listen}"))?;
            tracing::info!(%listen, hosts = ?proxy::PUSH_HOSTS, "egress proxy listening");
            tokio::select! {
                _ = proxy::serve(listener, proxy::Rules::push_hosts()) => {}
                _ = shutdown_signal() => {}
            }
            Ok(())
        })
}

/// What the binary was asked to do.
enum Command {
    /// Run the server.
    Serve {
        config: PathBuf,
        push: Option<PathBuf>,
    },
    /// Run the egress proxy.
    EgressProxy { listen: std::net::SocketAddr },
    /// Write a database snapshot to `to`.
    Backup { config: PathBuf, to: PathBuf },
}

fn command_from_args() -> Result<Command> {
    let mut args = std::env::args_os().skip(1).peekable();
    let sub = args.peek().and_then(|a| a.to_str()).map(str::to_owned);
    let backup = sub.as_deref() == Some("backup");
    let egress = sub.as_deref() == Some("egress-proxy");
    if backup || egress {
        args.next();
    }
    let mut config = None;
    let mut to = None;
    let mut push = None;
    let mut listen = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config") if !egress => config = Some(path_after(&mut args, "--config")?),
            Some("--to") if backup => to = Some(path_after(&mut args, "--to")?),
            Some("--push") if !backup && !egress => push = Some(path_after(&mut args, "--push")?),
            Some("--listen") if egress => {
                let text = path_after(&mut args, "--listen")?;
                listen =
                    Some(text.to_string_lossy().parse().with_context(|| {
                        format!("--listen {text:?} is not an address and port")
                    })?);
            }
            Some("--help" | "-h") => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ => bail!("unknown argument {arg:?}\n{USAGE}"),
        }
    }
    if egress {
        return Ok(Command::EgressProxy {
            listen: listen.context("missing --listen <addr>")?,
        });
    }
    let config = config.context("missing --config <file.toml>")?;
    Ok(if backup {
        Command::Backup {
            config,
            to: to.context("missing --to <file>")?,
        }
    } else {
        Command::Serve { config, push }
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
