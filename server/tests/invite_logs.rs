//! device-identity: Invites. The Pi logs an invite revoke like an invite
//! create: the kind of event and the username, never the code (qa-fixes D8).
//! This binary installs a global log capture, so it holds one test.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use support::harness::{TestPi, authed};
use support::{Check, bootstrap_admin, pb};

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn lines(&self) -> Vec<String> {
        let bytes = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

// spec: device-identity/invites
#[tokio::test]
async fn revoking_an_invite_is_logged_without_the_code() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).check("log capture");

    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let invite = conn
        .admin()
        .create_invite(authed(
            admin.token(),
            pb::CreateInviteRequest {
                username: "bob".to_owned(),
            },
        ))
        .await
        .check("CreateInvite")
        .into_inner()
        .invite
        .check("invite");
    conn.admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest {
                code: invite.code.clone(),
            },
        ))
        .await
        .check("RevokeInvite");

    let revoked: Vec<String> = captured
        .lines()
        .into_iter()
        .filter(|line| line.contains("revoked an invite"))
        .collect();
    assert_eq!(revoked.len(), 1, "{revoked:?}");
    let line = &revoked[0];
    assert!(line.contains("bob"), "{line}");
    assert!(!line.contains(&invite.code), "{line}");
}
