// app-ui: Safety and moderation entry (moderation: Published moderation policy,
// change `report-and-block`). The Profile tab's "Safety & moderation" entry
// opens this screen, which shows the published policy: how to report and block,
// that the admin reviews and acts within 24 hours, and the safety contact email
// (the <SAFETY_CONTACT_EMAIL> token until the owner's real address replaces it).
import 'package:flutter/material.dart';

import '../app/moderation.dart';

/// The published moderation policy, reachable from the Profile tab.
class SafetyModerationScreen extends StatelessWidget {
  const SafetyModerationScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        key: const Key('safety-screen'),
        title: const Text('Safety & moderation'),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'Safety & moderation policy',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 12),
          const Text(key: Key('safety-policy'), moderationPolicy),
        ],
      ),
    );
  }
}
