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

/// A chat list's time: the clock time today, "Yesterday", the weekday within
/// a week, else the short date.
String listTimeText(BuildContext context, int ms, {DateTime? now}) {
  final when = DateTime.fromMillisecondsSinceEpoch(ms);
  final today = now ?? DateTime.now();
  final l = MaterialLocalizations.of(context);
  DateTime day(DateTime t) => DateTime(t.year, t.month, t.day);
  final days = day(today).difference(day(when)).inDays;
  if (days <= 0) return l.formatTimeOfDay(TimeOfDay.fromDateTime(when));
  if (days == 1) return 'Yesterday';
  if (days < 7) return _weekdays[when.weekday - 1];
  return l.formatShortDate(when);
}

const _weekdays = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
