use protocol::message_content::{MAX_REPLY_QUOTE_CHARS, reply_quote};

// spec: message-content/replies
#[test]
fn short_text_is_quoted_whole_with_whitespace_folded() {
    assert_eq!(reply_quote("dinner at 8?"), "dinner at 8?");
    assert_eq!(reply_quote("  dinner\n\n at\t8? "), "dinner at 8?");
    assert_eq!(reply_quote(""), "");
}

// spec: message-content/replies
#[test]
fn long_text_is_cut_to_the_limit_with_an_ellipsis() {
    let long = "a".repeat(300);
    let quote = reply_quote(&long);
    assert_eq!(quote.chars().count(), MAX_REPLY_QUOTE_CHARS);
    assert!(quote.ends_with('…'));
    assert_eq!(quote, format!("{}…", "a".repeat(99)));
}

// spec: message-content/replies
#[test]
fn cut_counts_characters_not_bytes() {
    let long = "വ്യ".repeat(80); // 3 scalar values each, 240 in all
    let quote = reply_quote(&long);
    assert_eq!(quote.chars().count(), MAX_REPLY_QUOTE_CHARS);
    assert!(quote.ends_with('…'));
}

// spec: message-content/replies
#[test]
fn a_well_formed_quote_is_unchanged_by_a_second_pass() {
    let quote = reply_quote(&"word ".repeat(60));
    assert_eq!(reply_quote(&quote), quote);
    assert!(quote.chars().count() <= MAX_REPLY_QUOTE_CHARS);
}
