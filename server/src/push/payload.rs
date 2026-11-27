//! The wake push payload, built in one place for both providers
//! (message-delivery: Wake push payload is content-free and metadata-minimal).
//!
//! Anything in a push payload reaches Apple or Google, so it carries a wake and
//! nothing else: no message content, no sender, no `group_id`, no count. The
//! only text is the fixed fallback [`WAKE_TEXT`], shown if the device cannot
//! enrich the notification itself (local-notifications: The device builds the
//! notification locally).
//!
//! - iOS: a visible alert with that text and `mutable-content`, so a
//!   Notification Service Extension may rewrite it on the device.
//! - Android: a high-priority *data* message with no notification block; the
//!   app wakes and builds the notification.
//!
//! [`apns`] and [`fcm`] are the only places a payload is made, and both check
//! the result against the allow-list in [`is_wake_only`].

use serde_json::{Value, json};

use super::{COLLAPSE_KEY, WAKE_TEXT};

/// The one data key of the Android wake message.
pub const FCM_WAKE_KEY: &str = "wake";

/// The APNs body: an alert with the fixed text, `mutable-content`, and the
/// default sound. Nothing else.
pub fn apns() -> Value {
    let body = json!({
        "aps": {
            "alert": { "body": WAKE_TEXT },
            "sound": "default",
            "mutable-content": 1,
        }
    });
    debug_assert!(is_wake_only(&body), "APNs payload left the allow-list");
    body
}

/// The FCM HTTP v1 request body for `token`: a high-priority data message
/// (no `notification` block) that wakes the app. Nothing else.
pub fn fcm(token: &str) -> Value {
    let body = json!({
        "message": {
            "token": token,
            "data": { FCM_WAKE_KEY: "1" },
            "android": {
                "priority": "HIGH",
                "ttl": "86400s",
                "collapse_key": COLLAPSE_KEY,
            },
        }
    });
    debug_assert!(is_wake_only(&body), "FCM payload left the allow-list");
    body
}

/// Whether `payload` (an APNs body or an FCM request body) carries only the wake
/// mechanism and the fixed text: every leaf is on the allow-list, and no string
/// is anything but a known constant (or the device token). A new field or a
/// different string fails here, so content, a sender, a `group_id` or a count
/// cannot be added by accident.
pub fn is_wake_only(payload: &Value) -> bool {
    let mut leaves = Vec::new();
    collect(payload, String::new(), &mut leaves);
    leaves.iter().all(|(path, leaf)| allowed(path, leaf))
}

fn collect(value: &Value, path: String, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect(inner, next, out);
            }
        }
        // Arrays are never part of a wake payload.
        Value::Array(_) => out.push((path, value.clone())),
        _ => out.push((path, value.clone())),
    }
}

fn allowed(path: &str, leaf: &Value) -> bool {
    match (path, leaf) {
        ("aps.alert.body", Value::String(text)) => text == WAKE_TEXT,
        ("aps.sound", Value::String(sound)) => sound == "default",
        ("aps.mutable-content", Value::Number(n)) => n.as_u64() == Some(1),
        // The FCM device token is the one per-device string.
        ("message.token", Value::String(_)) => true,
        ("message.data.wake", Value::String(flag)) => flag == "1",
        ("message.android.priority", Value::String(p)) => p == "HIGH",
        ("message.android.ttl", Value::String(ttl)) => ttl == "86400s",
        ("message.android.collapse_key", Value::String(key)) => key == COLLAPSE_KEY,
        _ => false,
    }
}
