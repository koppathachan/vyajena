// app-ui: Emoji-only messages. A message that is nothing but a few emoji (a
// thumbs-up, a row of hearts) renders large — the "jumbomoji" effect other
// chat apps show — so a reaction-style message reads at a glance instead of
// as tiny glyphs at body-text size. Longer emoji runs and any message with
// letters, digits or other text stay at the ordinary size.
import 'package:flutter/widgets.dart';

/// Whitespace that may sit between emoji without making a message "not emoji".
bool _isWhitespace(int r) =>
    r == 0x20 || r == 0x09 || r == 0x0A || r == 0x0D || r == 0xA0;

/// Zero-width joiner, variation selectors and the combining keycap: the glue
/// that joins emoji sequences (family emoji, flags, ❤️). Not emoji on their
/// own, but allowed inside an emoji-only message.
bool _isJoiner(int r) =>
    r == 0x200D || r == 0xFE0E || r == 0xFE0F || r == 0x20E3;

/// Whether [r] is an emoji code point, across the pictographic blocks and the
/// older symbol/dingbat/arrow ranges emoji are drawn from.
bool _isEmojiRune(int r) =>
    (r >= 0x1F000 && r <= 0x1FAFF) || // pictographs, supplemental, flags, tones
    (r >= 0x2600 && r <= 0x27BF) || // misc symbols and dingbats
    (r >= 0x2B00 && r <= 0x2BFF) || // stars, arrows, shapes (⭐ ⬛)
    (r >= 0x2300 && r <= 0x23FF) || // ⌚ ⏰ ⏳ ⚙
    (r >= 0x2190 && r <= 0x21FF) || // arrows
    (r >= 0x25A0 && r <= 0x25FF) || // geometric shapes (▪ ◼ ◻)
    (r >= 0xE0000 && r <= 0xE007F) || // tag characters (subdivision flags)
    r == 0x24C2 || // Ⓜ
    r == 0x203C || // ‼
    r == 0x2049 || // ⁉
    r == 0x2122 || // ™
    r == 0x2139; // ℹ

/// Whether [text] is made up only of emoji (plus any joiners and spacing):
/// no letters, digits or other text. Empty or whitespace-only text is not.
bool isEmojiOnly(String text) {
  var sawEmoji = false;
  for (final r in text.runes) {
    if (_isWhitespace(r) || _isJoiner(r)) continue;
    if (_isEmojiRune(r)) {
      sawEmoji = true;
      continue;
    }
    return false;
  }
  return sawEmoji;
}

/// The most emoji a message is drawn large: beyond this it reads as a wall of
/// glyphs, so it stays at body size.
const _jumboMax = 8;

/// The font size for an emoji-only message [text], or null to draw it as
/// ordinary text. A single emoji is largest; the size steps down as the count
/// grows, and a run longer than [_jumboMax] is not enlarged at all. The count
/// is in grapheme clusters, so a joined sequence (a family, a flag) counts as
/// one emoji.
double? jumboEmojiSize(String text) {
  if (!isEmojiOnly(text)) return null;
  final count = text.characters.where((c) => c.trim().isNotEmpty).length;
  if (count == 0 || count > _jumboMax) return null;
  if (count <= 2) return 44;
  if (count <= 4) return 34;
  return 26;
}
