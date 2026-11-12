// A selectable block of text the user copies or shares: an invite, a link
// request or a link approval (flutter-app design D5).
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'platform.dart';

/// Shows [text] with Copy and Share buttons.
class ShareableText extends StatelessWidget {
  const ShareableText({super.key, required this.text});

  final String text;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Card.outlined(
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: SelectableText(text),
        ),
      ),
      const SizedBox(height: 8),
      Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          TextButton.icon(
            onPressed: () async {
              await Clipboard.setData(ClipboardData(text: text));
              if (context.mounted) {
                ScaffoldMessenger.of(
                  context,
                ).showSnackBar(const SnackBar(content: Text('Copied')));
              }
            },
            icon: const Icon(Icons.copy),
            label: const Text('Copy'),
          ),
          const SizedBox(width: 8),
          FilledButton.icon(
            onPressed: () => shareText(text),
            icon: const Icon(Icons.share),
            label: const Text('Share'),
          ),
        ],
      ),
    ],
  );
}
