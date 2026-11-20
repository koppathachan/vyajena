//! server-runtime: Egress proxy (push design D5). `vyajena-server egress-proxy
//! --listen 127.0.0.1:8445` tunnels HTTP `CONNECT` to the push hosts on port
//! 443 and nothing else, so `vyajena.service` itself stays loopback-only. It
//! only copies TLS bytes: the server's TLS runs end to end through it.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// The hosts the real proxy tunnels to, all on port 443.
pub const PUSH_HOSTS: &[&str] = &[
    "fcm.googleapis.com",
    "oauth2.googleapis.com",
    "api.push.apple.com",
    "api.sandbox.push.apple.com",
];

const MAX_HEAD: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A tunnel ends after this long whatever it does (the providers' pools drop
/// idle connections after 90 s anyway).
const MAX_TUNNEL: Duration = Duration::from_secs(3600);
const MAX_TUNNELS: usize = 64;

/// What the proxy lets through.
#[derive(Debug, Clone)]
pub struct Rules {
    /// `(host, port)` pairs, host in lower case.
    pub allowed: Vec<(String, u16)>,
    /// Refuse names that resolve to loopback, private, link-local, CGNAT or
    /// unspecified addresses (off only in tests, whose fakes are on loopback).
    pub refuse_private: bool,
}

impl Rules {
    /// The real allowlist: [`PUSH_HOSTS`] on 443, public addresses only.
    pub fn push_hosts() -> Self {
        Self {
            allowed: PUSH_HOSTS.iter().map(|h| ((*h).to_owned(), 443)).collect(),
            refuse_private: true,
        }
    }
}

/// Serves `listener` until the task is dropped.
pub async fn serve(listener: TcpListener, rules: Rules) {
    let rules = Arc::new(rules);
    let slots = Arc::new(Semaphore::new(MAX_TUNNELS));
    loop {
        let Ok((client, _)) = listener.accept().await else {
            continue;
        };
        let Ok(slot) = slots.clone().acquire_owned().await else {
            return;
        };
        let rules = rules.clone();
        tokio::spawn(async move {
            handle(client, &rules).await;
            drop(slot);
        });
    }
}

async fn handle(mut client: TcpStream, rules: &Rules) {
    let Ok(Some((head, rest))) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut client))
        .await
        .map(Result::ok)
        .map(Option::flatten)
    else {
        let _ = reply(&mut client, "400 Bad Request").await;
        return;
    };
    let target = match parse_connect(&head) {
        Err(status) => {
            tracing::warn!(status, "egress proxy refused a request");
            let _ = reply(&mut client, status).await;
            return;
        }
        Ok(target) => target,
    };
    let (host, port) = target;
    if !rules.allowed.iter().any(|(h, p)| *h == host && *p == port) {
        tracing::warn!(%host, port, "egress proxy refused a host outside its list");
        let _ = reply(&mut client, "403 Forbidden").await;
        return;
    }
    let addrs: Vec<SocketAddr> = match tokio::net::lookup_host((host.as_str(), port)).await {
        Ok(addrs) => addrs
            .filter(|a| !rules.refuse_private || !forbidden(a.ip()))
            .collect(),
        Err(_) => Vec::new(),
    };
    if addrs.is_empty() {
        tracing::warn!(%host, "egress proxy: no public address");
        let _ = reply(&mut client, "403 Forbidden").await;
        return;
    }
    let mut upstream = None;
    for addr in addrs {
        if let Ok(Ok(stream)) =
            tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await
        {
            upstream = Some(stream);
            break;
        }
    }
    let Some(mut upstream) = upstream else {
        let _ = reply(&mut client, "502 Bad Gateway").await;
        return;
    };
    if reply(&mut client, "200 Connection established")
        .await
        .is_err()
    {
        return;
    }
    if !rest.is_empty() && upstream.write_all(&rest).await.is_err() {
        return;
    }
    let _ = tokio::time::timeout(
        MAX_TUNNEL,
        tokio::io::copy_bidirectional(&mut client, &mut upstream),
    )
    .await;
}

/// Reads up to the blank line; returns the head and any bytes after it.
async fn read_head(client: &mut TcpStream) -> std::io::Result<Option<(String, Vec<u8>)>> {
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 1024];
    loop {
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(end + 4);
            return Ok(String::from_utf8(buf).ok().map(|head| (head, rest)));
        }
        if buf.len() > MAX_HEAD {
            return Ok(None);
        }
    }
}

/// `CONNECT host:port HTTP/1.x` → `(host, port)`; else the status to answer.
fn parse_connect(head: &str) -> Result<(String, u16), &'static str> {
    let line = head.lines().next().unwrap_or("");
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("400 Bad Request");
    };
    if !version.starts_with("HTTP/1.") {
        return Err("400 Bad Request");
    }
    if method != "CONNECT" {
        return Err("405 Method Not Allowed");
    }
    let (host, port) = target.rsplit_once(':').ok_or("400 Bad Request")?;
    let port: u16 = port.parse().map_err(|_| "400 Bad Request")?;
    Ok((host.to_ascii_lowercase(), port))
}

async fn reply(client: &mut TcpStream, status: &str) -> std::io::Result<()> {
    client
        .write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").as_bytes())
        .await
}

/// Loopback, private, link-local, CGNAT, unspecified, multicast and
/// documentation addresses: never a push host (no DNS rebinding to the LAN).
pub fn forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => forbidden_v4(v4),
            None => forbidden_v6(v6),
        },
    }
}

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
}

fn forbidden_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}
