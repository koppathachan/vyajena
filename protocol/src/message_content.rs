//! message-content: what both ends of a reply agree on (change `mentions`).

/// The most Unicode scalar values a `ReplyTo.quote` holds, "…" included
/// (message-content: Replies).
pub const MAX_REPLY_QUOTE_CHARS: usize = 100;

/// The quote of `text` for a `ReplyTo` (message-content: Replies): every run
/// of whitespace folded to one space, trimmed, and cut to
/// [`MAX_REPLY_QUOTE_CHARS`] with "…" as the last character when cut. The
/// sender builds it from the original; a receiver runs it again on the quote
/// it got, which leaves a well-formed quote unchanged.
pub fn reply_quote(text: &str) -> String {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.chars().count() <= MAX_REPLY_QUOTE_CHARS {
        return folded;
    }
    let mut cut: String = folded.chars().take(MAX_REPLY_QUOTE_CHARS - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}
