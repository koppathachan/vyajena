// app-ui: Link a device, the side of the device that holds the identity
// key: paste the new device's link request, approve it, and show the
// approval to paste back on the new device.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/text_box.dart';
import '../core/core_service.dart';

/// Approve a new device's link request.
class ApproveLinkScreen extends StatefulWidget {
  const ApproveLinkScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<ApproveLinkScreen> createState() => _ApproveLinkScreenState();
}

class _ApproveLinkScreenState extends State<ApproveLinkScreen> {
  final _request = TextEditingController();
  String? _approval;
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _request.dispose();
    super.dispose();
  }

  Future<void> _approve() async {
    final text = _request.text.trim();
    if (text.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
      _approval = null;
    });
    try {
      final approval = await widget.core.approveLink(text);
      if (mounted) setState(() => _approval = approval);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final approval = _approval;
    return Scaffold(
      appBar: AppBar(title: const Text('Link a new device')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const Text(
            'On the new device, choose "link this device" and paste its '
            'link request here.',
          ),
          const SizedBox(height: 12),
          TextField(
            key: const Key('request'),
            controller: _request,
            minLines: 2,
            maxLines: 4,
            decoration: const InputDecoration(
              labelText: 'Link request',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: _busy ? null : _approve,
              child: const Text('Approve'),
            ),
          ),
          if (_error case final error?)
            Text(
              error,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          if (approval != null) ...[
            const SizedBox(height: 16),
            const Text(
              'Paste this approval on the new device. It joins the group as '
              'your device once this phone adds it.',
            ),
            const SizedBox(height: 8),
            ShareableText(text: approval),
          ],
        ],
      ),
    );
  }
}
