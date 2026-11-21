// app-ui: Links in messages (change `links`, design D1, D3). Only explicit
// http:// and https:// URLs are links; a tap shows the full URL in a sheet and
// only Open hands it to the OS browser. Nothing is fetched here: no preview,
// no title, no connection until the user taps Open. Nothing stores or logs
// text.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

/// A URL found in a text: a UTF-16 range, as `MentionSpan` uses.
typedef LinkRange = ({int start, int end});

final _url = RegExp(r'https?://[^\s<>]+', caseSensitive: false);

/// Characters trimmed off the end of a match, so "see https://a.com." does
/// not swallow the full stop.
const _trailing = '.,)]}!?:;\'"';

/// The `http://` and `https://` URLs in [text], in order. Bare domains,
/// `www.`, e-mail addresses and other schemes are not links.
List<LinkRange> findLinks(String text) {
  final found = <LinkRange>[];
  for (final m in _url.allMatches(text)) {
    var end = m.end;
    while (end > m.start && _trailing.contains(text[end - 1])) {
      end--;
    }
    // Nothing after the scheme: not a link.
    final scheme = text.startsWith('https', m.start) ? 8 : 7;
    if (end - m.start <= scheme) continue;
    found.add((start: m.start, end: end));
  }
  return found;
}

/// The sheet of a tapped [url]: the full destination, with Open, Copy and
/// Cancel (design D3).
Future<void> showLinkSheet(BuildContext context, String url) =>
    showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (context) => _LinkSheet(url: url),
    );

class _LinkSheet extends StatefulWidget {
  const _LinkSheet({required this.url});

  final String url;

  @override
  State<_LinkSheet> createState() => _LinkSheetState();
}

class _LinkSheetState extends State<_LinkSheet> {
  bool _failed = false;

  Future<void> _open() async {
    final uri = Uri.tryParse(widget.url);
    var ok = false;
    if (uri != null) {
      try {
        ok = await launchUrl(uri, mode: LaunchMode.externalApplication);
      } on Object {
        ok = false;
      }
    }
    if (!mounted) return;
    if (ok) {
      Navigator.of(context).pop();
    } else {
      setState(() => _failed = true);
    }
  }

  Future<void> _copy() async {
    await Clipboard.setData(ClipboardData(text: widget.url));
    if (mounted) Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Flexible(
              child: SingleChildScrollView(
                child: Text(widget.url, key: const Key('link-url')),
              ),
            ),
            if (_failed) ...[
              const SizedBox(height: 8),
              Text(
                'Couldn\'t open this link',
                key: const Key('link-failed'),
                style: TextStyle(color: scheme.error),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  key: const Key('link-cancel'),
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                TextButton(
                  key: const Key('link-copy'),
                  onPressed: _copy,
                  child: const Text('Copy'),
                ),
                if (!_failed)
                  FilledButton(
                    key: const Key('link-open'),
                    onPressed: _open,
                    child: const Text('Open'),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
