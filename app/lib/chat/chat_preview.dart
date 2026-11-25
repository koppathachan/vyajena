// app-shell: Chats list. The one-line preview of a chat's newest message: its
// text, or a media glyph and a word for an image, a voice message or a video
// (video-messages). Built from the history the core already gives; nothing is
// stored here.
import 'package:flutter/material.dart';

import '../core/core_service.dart';

/// What a preview row shows: an optional media [icon], then [text].
class ChatPreview {
  const ChatPreview(this.text, {this.icon});

  final String text;
  final IconData? icon;
}

/// The preview of [message]. In a group [from] (the sender's short name) goes
/// in front of someone else's message; a DM passes none.
ChatPreview previewOf(ChatMessage message, {String? from}) {
  final lead = message.ownUser ? 'You: ' : (from == null ? '' : '$from: ');
  return switch (message.body) {
    ChatBody_Text(:final text) => ChatPreview('$lead${_oneLine(text)}'),
    ChatBody_Image(:final image) => ChatPreview(
      '$lead${_caption(image.caption)}',
      icon: Icons.image_outlined,
    ),
    ChatBody_Audio() => ChatPreview(
      '${lead}Voice message',
      icon: Icons.mic_none,
    ),
    ChatBody_Video() => ChatPreview(
      '${lead}Video',
      icon: Icons.videocam_outlined,
    ),
    ChatBody_Unsupported() => ChatPreview('${lead}Unsupported message'),
  };
}

String _oneLine(String text) => text.replaceAll(RegExp(r'\s+'), ' ').trim();

String _caption(String? caption) {
  final line = _oneLine(caption ?? '');
  return line.isEmpty ? 'Photo' : line;
}
