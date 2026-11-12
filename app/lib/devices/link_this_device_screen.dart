// app-ui: Link a device, the new device's side: show its link request to
// copy to the device that holds the identity key, then paste the approval
// that device shows.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/text_box.dart';
import '../core/core_service.dart';

/// Link this device to an existing account.
class LinkThisDeviceScreen extends StatefulWidget {
  const LinkThisDeviceScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<LinkThisDeviceScreen> createState() => _LinkThisDeviceScreenState();
}

class _LinkThisDeviceScreenState extends State<LinkThisDeviceScreen> {
  final _approval = TextEditingController();
  String? _request;
  String? _error;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    widget.core.linkRequest().then(
      (request) {
        if (mounted) setState(() => _request = request);
      },
      onError: (Object error) {
        if (mounted) setState(() => _error = errorText(error));
      },
    );
  }

  @override
  void dispose() {
    _approval.dispose();
    super.dispose();
  }

  Future<void> _complete() async {
    final text = _approval.text.trim();
    if (text.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await widget.core.completeLink(text);
      // The core's state turns ready; back to the home screen, which now
      // shows the group.
      if (mounted) Navigator.of(context).popUntil((route) => route.isFirst);
    } catch (error) {
      if (mounted) {
        setState(() {
          _busy = false;
          _error = errorText(error);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final request = _request;
    final text = Theme.of(context).textTheme;
    return Scaffold(
      appBar: AppBar(title: const Text('Link this device')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text('1. Copy this link request', style: text.titleMedium),
          const SizedBox(height: 4),
          const Text(
            'Paste it on the phone you joined with, under '
            'Menu › Link a new device.',
          ),
          const SizedBox(height: 8),
          if (request != null) ShareableText(text: request),
          const SizedBox(height: 24),
          Text('2. Paste the approval it shows', style: text.titleMedium),
          const SizedBox(height: 8),
          TextField(
            key: const Key('approval'),
            controller: _approval,
            minLines: 2,
            maxLines: 4,
            decoration: const InputDecoration(
              labelText: 'Approval',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: request == null || _busy ? null : _complete,
              child: Text(_busy ? 'Linking…' : 'Link'),
            ),
          ),
          if (_error case final error?)
            Text(
              error,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
        ],
      ),
    );
  }
}
