//! Named groups through the bridge (change `lobby-multi-group`): the lobby kind,
//! creating a group with a member picked, its name, the member list and
//! closing it.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::ChatKindInfo;
use support::Check;
use support::device::{self, user};
use support::server::TestServer;

// spec: app-ui/chat-list
// spec: app-ui/admin-group-screens
// spec: app-ui/named-group-screen
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_creates_names_and_closes_a_group_through_the_bridge() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let bob = device::member_of_group(&admin, "bob").await;
    let carol = device::member_of_group(&admin, "carol").await;
    admin
        .wait_until("the admin sees both", |c| {
            c.members().is_ok_and(|m| {
                m.iter().any(|d| d.user == user("bob")) && m.iter().any(|d| d.user == user("carol"))
            })
        })
        .await;
    assert!(admin.core.named_groups_available());
    assert_eq!(
        admin.core.chats().check("chats")[0].kind,
        ChatKindInfo::Lobby
    );

    let chat = admin
        .core
        .create_named_group(Some("Weekend".into()), vec![user("bob")])
        .await
        .check("create the group");
    let in_list = |c: &bridge::api::core::Core, closed_wanted: bool| {
        c.chats().is_ok_and(|chats| {
            chats.iter().any(|chat| {
                matches!(&chat.kind, ChatKindInfo::Group { name: Some(n), closed }
                    if n == "Weekend" && *closed == closed_wanted)
            })
        })
    };
    bob.wait_until("Bob has the group", |c| in_list(c, false))
        .await;
    assert!(
        carol
            .core
            .chats()
            .check("chats")
            .iter()
            .all(|c| !matches!(c.kind, ChatKindInfo::Group { .. }))
    );
    admin
        .wait_until("the member list", |c| {
            c.group_members(chat)
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;

    // Only the admin closes it; the lobby cannot be closed.
    let error = bob.core.close_group(chat).await.expect_err("not the admin");
    assert!(matches!(
        error.kind,
        CoreErrorKind::NotAdmin | CoreErrorKind::NoSuchChat
    ));
    let error = admin.core.close_group(0).await.expect_err("the lobby");
    assert_eq!(error.kind, CoreErrorKind::NoSuchChat);
    admin.core.close_group(chat).await.check("close");
    bob.wait_until("Bob sees it closed", |c| in_list(c, true))
        .await;
}
