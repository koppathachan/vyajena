//! Mention ranges between the core and Dart (mentions design D1). The wire
//! and the core count UTF-8 bytes; Dart strings count UTF-16 code units.
//! Mapping only: a range that does not fall on boundaries maps to `None`.

/// The UTF-16 `(start, length)` of the UTF-8 byte range `start..start + len`
/// of `text`; `None` if it is not on character boundaries inside `text`.
pub fn to_utf16(text: &str, start: usize, len: usize) -> Option<(u32, u32)> {
    let end = start.checked_add(len)?;
    let before = text.get(..start)?;
    let inside = text.get(start..end)?;
    let start16 = before.encode_utf16().count();
    let len16 = inside.encode_utf16().count();
    Some((u32::try_from(start16).ok()?, u32::try_from(len16).ok()?))
}

/// The UTF-8 byte `(start, length)` of the UTF-16 range
/// `start..start + len` of `text`; `None` if either end splits a character
/// or lies outside `text`.
pub fn to_utf8(text: &str, start: u32, len: u32) -> Option<(usize, usize)> {
    let start = usize::try_from(start).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    let mut units = 0usize;
    let mut byte_start = None;
    let mut byte_end = None;
    for (byte, ch) in text.char_indices() {
        if units == start {
            byte_start = Some(byte);
        }
        if units == end {
            byte_end = Some(byte);
        }
        units += ch.len_utf16();
    }
    if units == start {
        byte_start = Some(text.len());
    }
    if units == end {
        byte_end = Some(text.len());
    }
    let (byte_start, byte_end) = (byte_start?, byte_end?);
    Some((byte_start, byte_end.checked_sub(byte_start)?))
}
