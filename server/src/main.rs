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
//! - `server admin-reset --config <file.toml> [--passphrase <p>]` is the local
//!   break-glass reset for a locked-out admin (admin-recovery: Operator
//!   break-glass reset). It opens the configured database directly, marks the
//!   admin removed and revokes the admin's devices and unused invites, so the
//!   next start re-mints a bootstrap invite. Host access is the authority; it
//!   has no network path.

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
        Command::AdminReset { config, passphrase } => return admin_reset(config, passphrase),
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
       server admin-reset --config <file.toml> [--passphrase <p>]
       server egress-proxy --listen <addr>";

/// admin-recovery: Operator break-glass reset, run locally on the box. Opens the
/// configured database directly and marks the admin removed and
/// re-bootstrappable; the next start re-mints a bootstrap invite.
fn admin_reset(config: PathBuf, passphrase: Option<String>) -> Result<()> {
    let config = Config::load(&config)?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?
        .block_on(async {
            let report = server::runtime::admin_reset::run(
                &config,
                passphrase.as_deref(),
                std::time::SystemTime::now(),
            )
            .await
            .context("admin reset")?;
            tracing::info!(
                admin = %report.admin,
                devices_revoked = report.devices_revoked,
                invites_revoked = report.invites_revoked,
                lobby_reseeded = report.lobby_reseeded,
                members_to_rejoin = report.members_to_rejoin,
                "admin reset: the admin is removed and re-bootstrappable"
            );
            // OQ2 (operator guidance only, no behaviour change): the next start
            // re-mints one bootstrap invite; after the admin re-registers and
            // re-creates the lobby, the members are re-added with a fresh
            // Welcome and every member sees a "safety number changed" re-trust
            // prompt.
            tracing::info!(
                "restart the server to re-mint the admin's bootstrap invite; after the admin \
                 re-registers and re-creates the lobby, every member is re-added and will be \
                 asked to re-trust the admin's new safety number, so warn them out of band"
            );
            Ok::<(), anyhow::Error>(())
        })
}

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
    /// Break-glass reset of the admin (admin-recovery).
    AdminReset {
        config: PathBuf,
        passphrase: Option<String>,
    },
}

fn command_from_args() -> Result<Command> {
    let mut args = std::env::args_os().skip(1).peekable();
    let sub = args.peek().and_then(|a| a.to_str()).map(str::to_owned);
    let backup = sub.as_deref() == Some("backup");
    let egress = sub.as_deref() == Some("egress-proxy");
    let reset = sub.as_deref() == Some("admin-reset");
    if backup || egress || reset {
        args.next();
    }
    let mut config = None;
    let mut to = None;
    let mut push = None;
    let mut listen = None;
    let mut passphrase = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config") if !egress => config = Some(path_after(&mut args, "--config")?),
            Some("--to") if backup => to = Some(path_after(&mut args, "--to")?),
            Some("--passphrase") if reset => {
                passphrase = Some(string_after(&mut args, "--passphrase")?)
            }
            Some("--push") if !backup && !egress && !reset => {
                push = Some(path_after(&mut args, "--push")?)
            }
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
    } else if reset {
        Command::AdminReset { config, passphrase }
    } else {
        Command::Serve { config, push }
    })
}

/// The string value after `flag`.
fn string_after(args: &mut impl Iterator<Item = std::ffi::OsString>, flag: &str) -> Result<String> {
    match args.next() {
        Some(value) => value
            .into_string()
            .map_err(|_| anyhow::anyhow!("{flag} needs a text value")),
        None => bail!("{flag} needs a value"),
    }
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
