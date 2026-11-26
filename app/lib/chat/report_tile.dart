// app-ui: The admin sees reports (moderation: A report reaches the admin,
// change `report-and-block`). A report arrives as an ordinary DM whose text is
// a canonical report block; this tile presents it distinctly — the reported
// user, the reason, and the referenced (group) or quoted (DM) content — with a
// path to the existing remove-user screen for the admin.
import 'package:flutter/material.dart';

import '../app/routes.dart';
import '../core/core_service.dart';

/// Whether [text] is a report block (moderation: The admin sees reports).
bool isReportText(String text) => text.startsWith('[vyajena-report v1]');

/// One report row, shown in place of a plain message bubble.
class ReportTile extends StatefulWidget {
  const ReportTile({super.key, required this.core, required this.text});

  final CoreService core;

  /// The report `Content.Text` to parse and present.
  final String text;

  @override
  State<ReportTile> createState() => _ReportTileState();
}

class _ReportTileState extends State<ReportTile> {
  ReportView? _report;

  /// Whether this device's user is the admin: only then the Review & remove
  /// path (moderation: The admin acts on a report with the existing removal).
  bool _admin = false;

  @override
  void initState() {
    super.initState();
    widget.core.parseReport(widget.text).then((report) {
      if (mounted) setState(() => _report = report);
    });
    widget.core.isAdmin().then((admin) {
      if (mounted) setState(() => _admin = admin);
    });
  }

  @override
  Widget build(BuildContext context) {
    final report = _report;
    final scheme = Theme.of(context).colorScheme;
    // Until it is parsed (or if it does not parse) show the raw text, never a
    // blank: nothing is dropped.
    if (report == null) {
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
        child: Text(widget.text),
      );
    }
    final details = switch (report.kind) {
      ReportKindInfo_Dm(:final quote) => 'Quoted: “$quote”',
      ReportKindInfo_Group() => 'A message in a group you administer.',
      ReportKindInfo_User() => 'Reported this user.',
    };
    return Card(
      key: const Key('report-card'),
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      color: scheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.flag, size: 18, color: scheme.onErrorContainer),
                const SizedBox(width: 6),
                Text(
                  'Report',
                  style: Theme.of(context).textTheme.titleSmall?.copyWith(
                    color: scheme.onErrorContainer,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 6),
            Text(
              'Reported: ${report.reported.username}',
              style: TextStyle(color: scheme.onErrorContainer),
            ),
            if (report.reason.trim().isNotEmpty)
              Text(
                'Reason: ${report.reason}',
                style: TextStyle(color: scheme.onErrorContainer),
              ),
            const SizedBox(height: 4),
            Text(details, style: TextStyle(color: scheme.onErrorContainer)),
            if (_admin) ...[
              const SizedBox(height: 8),
              Align(
                alignment: Alignment.centerRight,
                child: FilledButton.tonal(
                  key: const Key('report-remove'),
                  onPressed: () =>
                      Navigator.of(context).pushNamed(Routes.removeUser),
                  child: const Text('Review & remove'),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
