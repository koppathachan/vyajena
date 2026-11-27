//! message-delivery: Wake push payload is content-free and metadata-minimal. The
//! serialized payloads both providers build carry only the wake and the generic
//! string, and the allow-list rejects anything else.

use serde_json::{Value, json};
use server::push::payload;

/// Every string and number anywhere in `value`, flattened.
fn leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
        Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
        other => out.push(other.to_string()),
    }
}

// spec: message-delivery/wake-push-payload-is-content-free-and-metadata-minimal
#[test]
fn ios_payload_carries_only_a_wake_and_the_generic_fallback() {
    let body = payload::apns();
    assert_eq!(
        body,
        json!({ "aps": {
            "alert": { "body": "New message" },
            "sound": "default",
            "mutable-content": 1,
        } })
    );
    assert!(payload::is_wake_only(&body));
    // No custom top-level key: nothing but `aps`.
    assert_eq!(body.as_object().map(|m| m.len()), Some(1));
}

// spec: message-delivery/wake-push-payload-is-content-free-and-metadata-minimal
#[test]
fn android_payload_is_a_data_message_with_no_notification_block() {
    let body = payload::fcm("bob-phone");
    assert_eq!(
        body,
        json!({ "message": {
            "token": "bob-phone",
            "data": { "wake": "1" },
            "android": { "priority": "HIGH", "ttl": "86400s", "collapse_key": "wake" },
        } })
    );
    assert!(payload::is_wake_only(&body));
    assert!(body["message"].get("notification").is_none());
    assert!(body["message"]["android"].get("notification").is_none());
}

// spec: message-delivery/wake-push-payload-is-content-free-and-metadata-minimal
#[test]
fn no_count_even_when_many_are_queued() {
    // The builders take no argument that could carry a number or a group: the
    // payload is the same whatever is queued, so the device counts for itself.
    assert_eq!(payload::apns(), payload::apns());
    assert_eq!(payload::fcm("t"), payload::fcm("t"));
    let mut all = Vec::new();
    leaves(&payload::apns(), &mut all);
    leaves(&payload::fcm("t"), &mut all);
    // The only number anywhere is the `mutable-content` flag.
    let numbers: Vec<_> = all.iter().filter(|l| l.parse::<f64>().is_ok()).collect();
    assert_eq!(numbers, ["1"]);
}

// spec: message-delivery/wake-push-payload-is-content-free-and-metadata-minimal
#[test]
fn the_allow_list_rejects_content_sender_group_id_and_count() {
    for bad in [
        json!({ "aps": { "alert": { "body": "Alice: hi" } } }),
        json!({ "aps": { "alert": { "body": "New message" }, "badge": 3 } }),
        json!({ "aps": { "alert": { "body": "New message" }, "thread-id": "abcd" } }),
        json!({ "aps": { "alert": { "body": "New message" } }, "group_id": "abcd" }),
        json!({ "aps": { "alert": { "body": "New message", "title": "Work" } } }),
        json!({ "message": { "token": "t", "data": { "wake": "1", "count": "5" } } }),
        json!({ "message": { "token": "t", "data": { "wake": "1", "sender": "alice" } } }),
        json!({ "message": { "token": "t", "notification": { "body": "New message" } } }),
        json!({ "message": { "token": "t", "data": { "wake": ["1"] } } }),
    ] {
        assert!(!payload::is_wake_only(&bad), "{bad}");
    }
}
