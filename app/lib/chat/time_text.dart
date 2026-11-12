// Times in the chat, with the stock Material localizations (no intl
// package).
import 'package:flutter/material.dart';

/// "Nov 12, 2026 4:05 PM" for [ms] since the Unix epoch, in local time.
String timeText(BuildContext context, int ms) {
  final when = DateTime.fromMillisecondsSinceEpoch(ms);
  final l = MaterialLocalizations.of(context);
  return '${l.formatMediumDate(when)} '
      '${l.formatTimeOfDay(TimeOfDay.fromDateTime(when))}';
}
