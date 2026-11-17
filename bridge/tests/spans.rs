//! Mention ranges between UTF-8 (core, wire) and UTF-16 (Dart), mentions
//! design D1.

use bridge::spans::{to_utf8, to_utf16};

// spec: message-content/mentions
#[test]
fn ascii_ranges_are_the_same_in_both() {
    assert_eq!(to_utf16("hi @Bob", 3, 4), Some((3, 4)));
    assert_eq!(to_utf8("hi @Bob", 3, 4), Some((3, 4)));
}

// spec: message-content/mentions
#[test]
fn emoji_and_malayalam_round_trip() {
    // "👋" is 4 UTF-8 bytes and 2 UTF-16 units; "വ്യ" 9 bytes and 3 units.
    let text = "👋 വ്യ @Ali";
    let start = text.find('@').expect("@");
    assert_eq!(start, 4 + 1 + 9 + 1);
    let (s16, l16) = to_utf16(text, start, 4).expect("utf16");
    assert_eq!((s16, l16), (2 + 1 + 3 + 1, 4));
    assert_eq!(to_utf8(text, s16, l16), Some((start, 4)));
    // A mention of the emoji itself.
    assert_eq!(to_utf16(text, 0, 4), Some((0, 2)));
    assert_eq!(to_utf8(text, 0, 2), Some((0, 4)));
}

// spec: message-content/mentions
#[test]
fn ranges_that_split_a_character_or_leave_the_text_map_to_none() {
    let text = "👋@a";
    assert_eq!(to_utf16(text, 1, 2), None); // inside the emoji
    assert_eq!(to_utf16(text, 4, 9), None); // past the end
    assert_eq!(to_utf8(text, 1, 1), None); // between the surrogates
    assert_eq!(to_utf8(text, 2, 9), None); // past the end
    assert_eq!(to_utf8(text, u32::MAX, 1), None);
    // The end of the text is a boundary.
    assert_eq!(to_utf8(text, 2, 2), Some((4, 2)));
    assert_eq!(to_utf16(text, 4, 2), Some((2, 2)));
}
