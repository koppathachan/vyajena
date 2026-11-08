//! device-identity: Invites (Admin.CreateInvite, ListInvites, RevokeInvite).

mod support;

use std::time::Duration;

use protocol::device_identity::INVITE_TTL;
use support::harness::{TestPi, authed};
use support::{Check, bootstrap_admin, create_invite, invite_user, pb};
use tonic::Code;

// spec: device-identity/invites
#[tokio::test]
async fn admin_creates_a_single_use_invite_bound_to_alice() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let response = create_invite(&conn, &admin, "alice").await;
    let invite = response.invite.check("invite");
    let alice = pb::UserId {
        username: "alice".to_owned(),
        server_id: "pi".to_owned(),
    };
    assert_eq!(invite.user_id, Some(alice.clone()));
    let created = invite.created_at.check("created_at").seconds;
    let expires = invite.expires_at.check("expires_at").seconds;
    assert_eq!(expires - created, INVITE_TTL.as_secs() as i64);

    // The payload carries the relay address, the TLS pin and the code.
    let payload = response.payload.check("payload");
    assert_eq!(payload.relay_address, "relay.example:443");
    assert_eq!(payload.tls_spki_sha256, pi.pin.to_vec());
    assert_eq!(payload.invite_code, invite.code);
    assert_eq!(payload.user_id, Some(alice));
}

// spec: device-identity/invites
#[tokio::test]
async fn non_admin_creating_an_invite_is_permission_denied() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (_, alice) = invite_user(&conn, &admin, "alice").await;
    let admin_rpcs = conn.admin();
    let denied = |result: Result<(), tonic::Status>| {
        assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    };
    denied(
        admin_rpcs
            .clone()
            .create_invite(authed(
                alice.token(),
                pb::CreateInviteRequest {
                    username: "carol".to_owned(),
                },
            ))
            .await
            .map(drop),
    );
    denied(
        admin_rpcs
            .clone()
            .list_invites(authed(alice.token(), pb::ListInvitesRequest {}))
            .await
            .map(drop),
    );
    denied(
        admin_rpcs
            .clone()
            .revoke_invite(authed(alice.token(), pb::RevokeInviteRequest::default()))
            .await
            .map(drop),
    );
    let invites: i64 = pi
        .query(|db| db.query_row("SELECT count(*) FROM invites", [], |r| r.get(0)))
        .await;
    assert_eq!(invites, 2, "the bootstrap invite and Alice's only");
}

// spec: device-identity/invites
#[tokio::test]
async fn admin_lists_and_revokes_unused_invites() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let bob = create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("invite");
    let carol = create_invite(&conn, &admin, "carol")
        .await
        .invite
        .check("invite");
    let (_, _dave) = invite_user(&conn, &admin, "dave").await;
    let list = |token: String| {
        let mut client = conn.admin();
        async move {
            client
                .list_invites(authed(&token, pb::ListInvitesRequest {}))
                .await
                .check("ListInvites")
                .into_inner()
                .invites
                .into_iter()
                .map(|i| i.code)
                .collect::<std::collections::BTreeSet<_>>()
        }
    };
    // Used invites (the bootstrap one and Dave's) are not listed.
    assert_eq!(
        list(admin.token().to_owned()).await,
        [bob.code.clone(), carol.code.clone()].into()
    );

    conn.admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest {
                code: bob.code.clone(),
            },
        ))
        .await
        .check("RevokeInvite");
    assert_eq!(
        list(admin.token().to_owned()).await,
        [carol.code.clone()].into()
    );

    let unknown = conn
        .admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest {
                code: "nope".to_owned(),
            },
        ))
        .await;
    assert_eq!(unknown.err().map(|s| s.code()), Some(Code::NotFound));

    // Expired invites drop off the list.
    pi.clock.advance(INVITE_TTL + Duration::from_secs(1));
    let mut admin = admin;
    admin.authenticate(&conn).await;
    assert!(list(admin.token().to_owned()).await.is_empty());
}

// spec: device-identity/invites
#[tokio::test]
async fn used_invite_cannot_be_revoked_and_existing_users_get_no_invite() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let used = conn
        .admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest {
                code: pi.bootstrap_code.clone(),
            },
        ))
        .await;
    assert_eq!(used.err().map(|s| s.code()), Some(Code::FailedPrecondition));

    let existing = conn
        .admin()
        .create_invite(authed(
            admin.token(),
            pb::CreateInviteRequest {
                username: "akhil".to_owned(),
            },
        ))
        .await;
    assert_eq!(existing.err().map(|s| s.code()), Some(Code::AlreadyExists));

    for bad in ["", "a@b", "has space"] {
        let result = conn
            .admin()
            .create_invite(authed(
                admin.token(),
                pb::CreateInviteRequest {
                    username: bad.to_owned(),
                },
            ))
            .await;
        assert_eq!(
            result.err().map(|s| s.code()),
            Some(Code::InvalidArgument),
            "{bad:?}"
        );
    }
}
