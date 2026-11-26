//! The report payload (moderation: Report a message; design D2, change
//! `report-and-block`).
//!
//! A report is an ordinary DM `Content.Text` whose body is a canonical,
//! machine-recognisable block: the reporter's core [`compose`]s it and sends it
//! to the admin over the existing DM path, and the admin's app [`parse`]s it to
//! show the report distinctly from an ordinary message. No wire message is
//! added, so on an older admin app the block still reads as plain text.
//!
//! A group/lobby report carries a *reference* (the admin already holds the
//! message): the reported user, the group id and the offending message's wire
//! id. A DM report carries a *quote* of the offending text (the admin is not in
//! the DM), folded and capped by the reply-quote rules; no DM key or ciphertext
//! leaves the reporter's device.

use crate::events::UserId;

/// The marker line that opens a report payload.
const HEADER: &str = "[vyajena-report v1]";

/// What a report points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportKind {
    /// A message in a group or the lobby: the admin already holds it, so the
    /// report references it by group id and wire message id.
    Group {
        /// The MLS group id of the group or lobby.
        group_id: Vec<u8>,
        /// The offending message's wire `msg_id`.
        msg_id: Vec<u8>,
    },
    /// A message in a DM: the admin is not in the DM, so the report quotes the
    /// offending text (reply-quote rules).
    Dm {
        /// The capped quote of the offending message's text.
        quote: String,
    },
    /// A whole user, with no message reference or quote.
    User,
}

/// A report, as the reporter composes it and the admin reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The reported user.
    pub reported: UserId,
    /// The reporter's short reason.
    pub reason: String,
    /// What the report points at.
    pub kind: ReportKind,
}

/// Composes the canonical report text an admin DM carries (design D2).
pub fn compose(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str("reported: ");
    out.push_str(&report.reported.to_string());
    out.push('\n');
    out.push_str("reason: ");
    out.push_str(&one_line(&report.reason));
    out.push('\n');
    match &report.kind {
        ReportKind::Group { group_id, msg_id } => {
            out.push_str("kind: group\n");
            out.push_str("group: ");
            out.push_str(&hex(group_id));
            out.push('\n');
            out.push_str("msg: ");
            out.push_str(&hex(msg_id));
            out.push('\n');
        }
        ReportKind::Dm { quote } => {
            out.push_str("kind: dm\n");
            out.push_str("quote: ");
            out.push_str(&one_line(quote));
            out.push('\n');
        }
        ReportKind::User => {
            out.push_str("kind: user\n");
        }
    }
    out
}

/// Parses a report block, or `None` when `text` is an ordinary message
/// (anything that is not a well-formed report).
pub fn parse(text: &str) -> Option<Report> {
    let mut lines = text.lines();
    if lines.next()?.trim() != HEADER {
        return None;
    }
    let mut reported = None;
    let mut reason = String::new();
    let mut kind_tag = None;
    let mut group_id = None;
    let mut msg_id = None;
    let mut quote = String::new();
    for line in lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "reported" => reported = parse_user(value),
            "reason" => reason = value.to_string(),
            "kind" => kind_tag = Some(value.to_string()),
            "group" => group_id = unhex(value),
            "msg" => msg_id = unhex(value),
            "quote" => quote = value.to_string(),
            _ => {}
        }
    }
    let kind = match kind_tag.as_deref()? {
        "group" => ReportKind::Group {
            group_id: group_id?,
            msg_id: msg_id?,
        },
        "dm" => ReportKind::Dm { quote },
        "user" => ReportKind::User,
        _ => return None,
    };
    Some(Report {
        reported: reported?,
        reason,
        kind,
    })
}

/// Parses `<username>@<server_id>`; `None` when either part is missing.
fn parse_user(text: &str) -> Option<UserId> {
    let (name, server) = text.split_once('@')?;
    if name.is_empty() || server.is_empty() {
        return None;
    }
    Some(UserId::new(name, server))
}

/// Folds every run of whitespace to one space and trims, so a field is one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0xf) as u32, 16).unwrap_or('0'));
    }
    out
}

/// Decodes lowercase/uppercase hex; `None` on an odd length or a non-hex digit.
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}
