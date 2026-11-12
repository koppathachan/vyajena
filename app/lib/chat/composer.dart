// app-ui: Composer limit. Text whose encoded content is over the core's
// MAX_CONTENT_BYTES is refused before sending (`textFits`); the text stays
// in the composer.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';

/// What the composer says about text that does not fit.
const tooLongText = 'Too long to send. Make it shorter.';

/// The message box and Send button.
class Composer extends StatefulWidget {
  const Composer({super.key, required this.core, required this.send});

  final CoreService core;

  /// Sends the text (stores it in the core).
  final Future<void> Function(String text) send;

  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  final _text = TextEditingController();
  String? _error;
  bool _sending = false;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _send() async {
    final text = _text.text;
    if (text.trim().isEmpty || _sending) return;
    if (!widget.core.textFits(text)) {
      setState(() => _error = tooLongText);
      return;
    }
    setState(() {
      _error = null;
      _sending = true;
    });
    try {
      await widget.send(text);
      _text.clear();
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  @override
  Widget build(BuildContext context) => SafeArea(
    top: false,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(12, 4, 8, 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Expanded(
            child: TextField(
              key: const Key('composer'),
              controller: _text,
              minLines: 1,
              maxLines: 6,
              textCapitalization: TextCapitalization.sentences,
              decoration: InputDecoration(
                hintText: 'Message',
                border: const OutlineInputBorder(),
                isDense: true,
                errorText: _error,
                errorMaxLines: 3,
              ),
              onChanged: (_) {
                if (_error != null) setState(() => _error = null);
              },
            ),
          ),
          const SizedBox(width: 4),
          IconButton.filled(
            tooltip: 'Send',
            onPressed: _sending ? null : _send,
            icon: const Icon(Icons.send),
          ),
        ],
      ),
    ),
  );
}
