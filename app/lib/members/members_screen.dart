// app-ui: Profile and members. The group's members by user, each with the
// number of devices they have in the group.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/names.dart';
import '../core/core_service.dart';

/// A member user and their devices in the group.
class MemberUser {
  MemberUser(this.user, this.devices);
  final UserRef user;
  final List<String> devices;
}

/// The group's member devices grouped by user, in the order the core lists
/// them.
List<MemberUser> byUser(List<MemberDevice> devices) {
  final users = <String, MemberUser>{};
  for (final d in devices) {
    final key = '${d.user.username}@${d.user.serverId}';
    users.putIfAbsent(key, () => MemberUser(d.user, [])).devices.add(d.device);
  }
  return users.values.toList();
}

/// "1 device", "2 devices".
String devicesText(int n) => n == 1 ? '1 device' : '$n devices';

/// The members list.
class MembersScreen extends StatefulWidget {
  const MembersScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<MembersScreen> createState() => _MembersScreenState();
}

class _MembersScreenState extends State<MembersScreen> {
  late final UserNames _names = UserNames(widget.core);
  List<MemberUser>? _members;
  String? _error;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      if (event case CoreEvent_ProfileChanged(:final user)) {
        _names.forget(user);
        _load();
      } else if (event is CoreEvent_MembersChanged) {
        _load();
      }
    });
    _load();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final members = byUser(await widget.core.members());
      await _names.load(members.map((m) => m.user));
      if (mounted) setState(() => _members = members);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  @override
  Widget build(BuildContext context) {
    final members = _members;
    final me = widget.core.user.value;
    return Scaffold(
      appBar: AppBar(title: const Text('Members')),
      body: switch ((members, _error)) {
        (_, final String error) => Center(child: Text(error)),
        (null, _) => const SizedBox.shrink(),
        (final List<MemberUser> members, _) => ListView(
          children: [
            for (final m in members)
              ListTile(
                leading: const Icon(Icons.person_outline),
                title: Text(
                  me != null &&
                          me.username == m.user.username &&
                          me.serverId == m.user.serverId
                      ? '${_names.of(m.user)} · you'
                      : _names.of(m.user),
                ),
                subtitle: Text(devicesText(m.devices.length)),
              ),
          ],
        ),
      },
    );
  }
}
