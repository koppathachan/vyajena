//! pi-runtime: Egress proxy. Only CONNECT, only the listed host and port,
//! never to a private address; the providers reach FCM through it.

mod push_support;

use std::net::IpAddr;

use push_support::{FakeServer, credentials_dir, options, service_account_json, write_key};
use server::pb;
use server::push::proxy::{self, Rules};
use server::push::{Credentials, Providers, PushProvider, PushSettings, WakeOutcome};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn start(rules: Rules) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(proxy::serve(listener, rules));
    addr
}

/// Sends `request` and returns the status line.
async fn ask(proxy: std::net::SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(proxy).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut buf = vec![0; 256];
    let n = stream.read(&mut buf).await.expect("read");
    String::from_utf8_lossy(&buf[..n])
        .lines()
        .next()
        .unwrap_or("")
        .to_owned()
}

// spec: pi-runtime/egress-proxy
#[tokio::test]
async fn compromised_server_dials_out_and_is_refused() {
    let fake = FakeServer::start().await;
    let port = fake.addr.port();
    let addr = start(Rules {
        allowed: vec![("localhost".into(), port)],
        refuse_private: false,
    })
    .await;
    assert_eq!(
        ask(
            addr,
            &format!("CONNECT localhost:{port} HTTP/1.1\r\nHost: x\r\n\r\n")
        )
        .await,
        "HTTP/1.1 200 Connection established"
    );
    for (request, want) in [
        (
            "CONNECT attacker.example:443 HTTP/1.1\r\n\r\n".to_owned(),
            "HTTP/1.1 403 Forbidden",
        ),
        (
            format!("CONNECT localhost:{} HTTP/1.1\r\n\r\n", port + 1),
            "HTTP/1.1 403 Forbidden",
        ),
        (
            format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n"),
            "HTTP/1.1 403 Forbidden",
        ),
        (
            format!("GET https://localhost:{port}/ HTTP/1.1\r\n\r\n"),
            "HTTP/1.1 405 Method Not Allowed",
        ),
        ("nonsense\r\n\r\n".to_owned(), "HTTP/1.1 400 Bad Request"),
    ] {
        assert_eq!(ask(addr, &request).await, want, "{request:?}");
    }
    assert!(fake.requests().is_empty());
}

// spec: pi-runtime/egress-proxy
#[tokio::test]
async fn a_listed_name_that_resolves_to_the_lan_is_refused() {
    let fake = FakeServer::start().await;
    let port = fake.addr.port();
    // Listed, but "localhost" is loopback: refused like a rebinding to 192.168.1.1.
    let addr = start(Rules {
        allowed: vec![("localhost".into(), port)],
        refuse_private: true,
    })
    .await;
    assert_eq!(
        ask(addr, &format!("CONNECT localhost:{port} HTTP/1.1\r\n\r\n")).await,
        "HTTP/1.1 403 Forbidden"
    );
    for ip in [
        "192.168.1.1",
        "10.0.0.1",
        "172.16.5.5",
        "127.0.0.1",
        "169.254.1.1",
        "100.64.0.1",
        "0.0.0.0",
        "::1",
        "fe80::1",
        "fd00::1",
        "::ffff:192.168.1.1",
    ] {
        assert!(proxy::forbidden(ip.parse::<IpAddr>().expect("ip")), "{ip}");
    }
    for ip in ["142.250.80.10", "17.188.143.1", "2607:f8b0:4004:c1b::5f"] {
        assert!(!proxy::forbidden(ip.parse::<IpAddr>().expect("ip")), "{ip}");
    }
    let real = Rules::push_hosts();
    assert!(real.refuse_private);
    assert_eq!(
        real.allowed,
        [
            ("fcm.googleapis.com".to_owned(), 443),
            ("oauth2.googleapis.com".to_owned(), 443),
            ("api.push.apple.com".to_owned(), 443),
            ("api.sandbox.push.apple.com".to_owned(), 443),
        ]
    );
}

// spec: pi-runtime/egress-proxy
#[tokio::test]
async fn fcm_pings_go_through_the_proxy() {
    let fake = FakeServer::start().await;
    let addr = start(Rules {
        allowed: vec![("localhost".into(), fake.addr.port())],
        refuse_private: false,
    })
    .await;
    let dir = credentials_dir();
    write_key(
        dir.path(),
        "fcm-service-account.json",
        &service_account_json(&fake),
    );
    let settings = PushSettings::from_toml(
        &format!(
            "credentials_dir = \".\"\nproxy = \"http://{addr}\"\napns_team_id = \"XMVHK3YASX\"\napns_topic = \"com.purplechaamba.vyajena\"\n"
        ),
        dir.path(),
    )
    .expect("settings");
    let push = Providers::new(
        Credentials::load(dir.path()).expect("creds"),
        &settings,
        &options(&fake, &fake, &fake),
    )
    .expect("providers");
    let token = pb::PushToken {
        platform: pb::PushPlatform::Fcm as i32,
        token: "t".into(),
    };
    assert_eq!(push.wake([1; 16], token).await, WakeOutcome::Sent);
    assert_eq!(
        fake.requests().len(),
        2,
        "sign-in and send, through the tunnel"
    );

    // With the proxy refusing the host, nothing gets out.
    let closed = start(Rules {
        allowed: vec![],
        refuse_private: false,
    })
    .await;
    let settings = PushSettings {
        proxy: Some(format!("http://{closed}")),
        ..settings
    };
    let fake2 = FakeServer::start().await;
    let push = Providers::new(
        Credentials::load(dir.path()).expect("creds"),
        &settings,
        &options(&fake2, &fake2, &fake2),
    )
    .expect("providers");
    let token = pb::PushToken {
        platform: pb::PushPlatform::Fcm as i32,
        token: "t".into(),
    };
    assert_eq!(push.wake([1; 16], token).await, WakeOutcome::Dropped);
    assert!(fake2.requests().is_empty());
}
