//! device-identity: Bootstrap admin.

mod support;

use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use rusqlite::params;
use server::runtime::{Bootstrap, Config, ManualClock, Server, StartError};
use server::store::Store;

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temp dir: {error}"),
    }
}

async fn start(config: &Config, clock: &ManualClock) -> Server {
    match Server::start(config.clone(), Arc::new(clock.clone())).await {
        Ok(server) => server,
        Err(error) => panic!("server did not start: {error}"),
    }
}

async fn bootstrap_rows(store: &Store) -> Vec<(String, String)> {
    let rows = store
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT code, username FROM invites WHERE bootstrap = 1 ORDER BY created_at",
            )?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await;
    rows.unwrap_or_default()
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn first_start_creates_one_bootstrap_invite_for_the_admin() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    let server = start(&config, &clock).await;

    let Bootstrap::Created(invite) = server.bootstrap().clone() else {
        panic!(
            "expected a new bootstrap invite, got {:?}",
            server.bootstrap()
        );
    };
    assert_eq!(invite.invite.username, "akhil");
    assert!(invite.invite.bootstrap);
    assert_eq!(invite.invite.created_at, support::test_epoch());
    assert_eq!(
        invite.invite.expires_at,
        support::test_epoch() + protocol::device_identity::INVITE_TTL
    );
    // 128 random bits, base64url without padding.
    assert_eq!(invite.invite.code.len(), 22);

    // The payload holds the relay address, the TLS pin, the code and the
    // admin's user id.
    let payload = &invite.payload;
    assert_eq!(payload.relay_address, "relay.example:443");
    assert_eq!(
        payload.tls_spki_sha256,
        server.state().keys.tls_spki_sha256().to_vec()
    );
    assert_eq!(payload.invite_code, invite.invite.code);
    let user = payload.user_id.clone().unwrap_or_default();
    assert_eq!(
        (user.username.as_str(), user.server_id.as_str()),
        ("akhil", "chat")
    );

    let rows = bootstrap_rows(&server.state().store).await;
    assert_eq!(rows, vec![(invite.invite.code.clone(), "akhil".to_owned())]);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn printed_payload_decodes_with_the_client_stubs() {
    use base64::Engine as _;
    let tmp = temp();
    let server = start(&support::config_in(tmp.path()), &support::manual_clock()).await;
    let Bootstrap::Created(invite) = server.bootstrap().clone() else {
        panic!("expected a new bootstrap invite");
    };

    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(invite.payload_text())
        .unwrap_or_default();
    let decoded = support::pb::InvitePayload::decode(bytes.as_slice()).ok();
    let decoded = decoded.unwrap_or_default();
    assert_eq!(decoded.invite_code, invite.invite.code);
    assert_eq!(decoded.relay_address, "relay.example:443");
    assert_eq!(decoded.tls_spki_sha256.len(), 32);
    assert_eq!(
        decoded
            .user_id
            .map(|u| format!("{}@{}", u.username, u.server_id)),
        Some("akhil@chat".to_owned())
    );
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn restart_before_signup_prints_no_second_invite() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    drop(start(&config, &clock).await);

    clock.advance(Duration::from_secs(60 * 60));
    let server = start(&config, &clock).await;
    assert!(
        matches!(server.bootstrap(), Bootstrap::Pending { .. }),
        "{:?}",
        server.bootstrap()
    );
    assert_eq!(bootstrap_rows(&server.state().store).await.len(), 1);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn expired_unused_bootstrap_invite_is_replaced_at_next_start() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    drop(start(&config, &clock).await);

    clock.advance(protocol::device_identity::INVITE_TTL + Duration::from_secs(1));
    let server = start(&config, &clock).await;
    assert!(
        matches!(server.bootstrap(), Bootstrap::Created(_)),
        "{:?}",
        server.bootstrap()
    );
    assert_eq!(bootstrap_rows(&server.state().store).await.len(), 2);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn no_bootstrap_invite_once_users_exist() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    let server = start(&config, &clock).await;
    let inserted = server
        .state()
        .store
        .call(|conn| {
            conn.execute(
                "INSERT INTO users (username, identity_public_key, created_at) VALUES (?1, ?2, ?3)",
                params!["akhil", vec![0u8; 32], 0_i64],
            )?;
            Ok(())
        })
        .await;
    assert!(inserted.is_ok());
    drop(server);

    // Even after the first invite expired, nothing new is created.
    clock.advance(protocol::device_identity::INVITE_TTL * 2);
    let server = start(&config, &clock).await;
    assert_eq!(server.bootstrap(), &Bootstrap::NotNeeded);
    assert_eq!(bootstrap_rows(&server.state().store).await.len(), 1);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn healthy_server_with_other_users_does_not_re_bootstrap() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    let server = start(&config, &clock).await;
    // The admin is active (removed_at NULL) and another user exists.
    let inserted = server
        .state()
        .store
        .call(|conn| {
            conn.execute(
                "INSERT INTO users (username, identity_public_key, created_at) VALUES (?1, ?2, ?3)",
                params!["akhil", vec![1u8; 32], 0_i64],
            )?;
            conn.execute(
                "INSERT INTO users (username, identity_public_key, created_at) VALUES (?1, ?2, ?3)",
                params!["bob", vec![2u8; 32], 0_i64],
            )?;
            Ok(())
        })
        .await;
    assert!(inserted.is_ok());
    drop(server);

    clock.advance(Duration::from_secs(60 * 60));
    let server = start(&config, &clock).await;
    assert_eq!(server.bootstrap(), &Bootstrap::NotNeeded);
    assert_eq!(bootstrap_rows(&server.state().store).await.len(), 1);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn re_bootstrap_after_the_admin_is_removed() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    let server = start(&config, &clock).await;
    // The admin registered (bootstrap invite consumed) and another user joined;
    // then an operator reset marked the admin removed.
    let prepared = server
        .state()
        .store
        .call(|conn| {
            conn.execute(
                "INSERT INTO users (username, identity_public_key, created_at, removed_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params!["akhil", vec![1u8; 32], 0_i64, 1_i64],
            )?;
            conn.execute(
                "INSERT INTO users (username, identity_public_key, created_at) VALUES (?1, ?2, ?3)",
                params!["bob", vec![2u8; 32], 0_i64],
            )?;
            // The first-start bootstrap invite has been used, so none is usable.
            conn.execute(
                "UPDATE invites SET used_at = ?1 WHERE bootstrap = 1",
                params![1_i64],
            )?;
            Ok(())
        })
        .await;
    assert!(prepared.is_ok());
    drop(server);

    clock.advance(Duration::from_secs(60 * 60));
    let server = start(&config, &clock).await;
    assert!(
        matches!(server.bootstrap(), Bootstrap::Created(invite) if invite.invite.username == "akhil"),
        "expected a re-minted bootstrap invite for the admin, got {:?}",
        server.bootstrap()
    );
    // A second bootstrap invite now exists (the used one plus the new one).
    assert_eq!(bootstrap_rows(&server.state().store).await.len(), 2);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn database_of_another_server_id_is_refused() {
    let tmp = temp();
    let config = support::config_in(tmp.path());
    let clock = support::manual_clock();
    drop(start(&config, &clock).await);

    let mut other = config.clone();
    other.server_id = "other-server".into();
    let result = Server::start(other, Arc::new(clock)).await;
    assert!(
        matches!(result, Err(StartError::ServerIdMismatch { .. })),
        "{:?}",
        result.err()
    );
}
