//! Text forms of invites, link requests and link approvals (flutter-app
//! design D5, task 2.3), and linking a device by text end to end.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::text::{INVITE_PREFIX, LINK_APPROVAL_PREFIX, LINK_REQUEST_PREFIX, parse_invite};
use bridge::api::types::CoreEvent;
use support::Check;
use support::device::{self, Device, user};
use support::pi::{ADMIN, TestPi};

// spec: app-ui/link-a-device
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_links_a_tablet_by_text() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;

    let mut tablet = Device::new().await;
    let request = tablet.core.link_request().await.check("link request");
    assert!(request.starts_with(LINK_REQUEST_PREFIX), "{request}");

    // Pasted with stray whitespace around it, as a share sheet may.
    let approval = admin
        .core
        .approve_link(format!("  {request}\n"))
        .await
        .check("approve");
    assert!(approval.starts_with(LINK_APPROVAL_PREFIX), "{approval}");

    let id = tablet
        .core
        .complete_link(approval)
        .await
        .check("complete link");
    assert_eq!(id, user(ADMIN));
    tablet.core.start().await.check("start tablet");
    tablet
        .wait_event("joined", |e| matches!(e, CoreEvent::JoinedGroup))
        .await;
    let devices = admin
        .core
        .members()
        .check("members")
        .into_iter()
        .filter(|m| m.user == user(ADMIN))
        .count();
    assert_eq!(devices, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_text_form_is_refused_where_another_kind_belongs() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let invite = admin
        .core
        .create_invite("bob".to_owned())
        .await
        .check("invite")
        .text
        .check("invite text");
    assert!(invite.starts_with(INVITE_PREFIX));

    let tablet = Device::new().await;
    let request = tablet.core.link_request().await.check("link request");

    // A link request where an approval belongs, and an invite where a link
    // request belongs: refused by kind, before any call to the Pi.
    let error = tablet
        .core
        .complete_link(request.clone())
        .await
        .expect_err("a request is not an approval");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);
    assert!(error.message.contains("link request"), "{}", error.message);
    let error = admin
        .core
        .approve_link(invite.clone())
        .await
        .expect_err("an invite is not a request");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);
    assert!(error.message.contains("invite"), "{}", error.message);
    let error = parse_invite(request).expect_err("a request is not an invite");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);

    // Garbage after a correct prefix.
    let error = admin
        .core
        .approve_link(format!("{LINK_REQUEST_PREFIX}!!!"))
        .await
        .expect_err("not base64url");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);

    // The invite text still works where it belongs.
    assert_eq!(parse_invite(invite).check("parse").user, user("bob"));
}
