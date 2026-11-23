// app-shell: the Chats list coalesces a burst of core events into one load.
import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// A core that counts the list's reads, and holds the first preview fetch
/// open (returning what it saw when it started) until [release] is called.
class CountingCore extends FakeCoreService {
  CountingCore() : super(storedUser: akhil);

  /// The list's newest-message fetches (limit 1): one per chat per load. The
  /// conversation models also read chats() and history, so those are not
  /// counted.
  int previewCalls = 0;
  Completer<void>? gate;

  void release() => gate?.complete();

  @override
  Future<List<ChatMessage>> historyIn(int chatId, int limit) async {
    if (limit == 1) previewCalls++;
    final snapshot = await super.historyIn(chatId, limit);
    final held = gate;
    if (held != null && !held.isCompleted) await held.future;
    return snapshot;
  }
}

void main() {
  // spec: app-shell/chats-list
  testWidgets('a burst of events during a load is one load plus one rerun, '
      'and nothing is dropped', (tester) async {
    final core = CountingCore();
    core.receive(alice, 'first', emitEvent: false);
    await openHome(tester, core);
    expect(find.text('alice: first'), findsOneWidget);
    final before = core.previewCalls;

    // The next load stalls on its preview fetch, holding a stale snapshot.
    core.gate = Completer<void>();
    core.emit(const CoreEvent.chatsChanged());
    await tester.pump();
    expect(core.previewCalls - before, 1);

    // A burst arrives while that load is in flight.
    core.receive(alice, 'second');
    core.emit(const CoreEvent.sendStatus(id: 1, status: SendState.accepted()));
    core.emit(const CoreEvent.chatsChanged());
    core.receive(alice, 'third');
    core.emit(const CoreEvent.chatsChanged());
    await tester.pump();
    // Held back: no extra load started for any of the five events.
    expect(core.previewCalls - before, 1);

    core.release();
    await settle(tester);

    // One in-flight load plus at most one rerun, not one per event.
    expect(core.previewCalls - before, 2);
    // The rerun saw the latest message: the stale snapshot did not win.
    expect(find.text('alice: third'), findsOneWidget);
    expect(find.text('alice: first'), findsNothing);
  });
}
