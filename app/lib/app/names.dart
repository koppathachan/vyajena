// How users are shown: "Ali (alice)", or the username alone, from the
// core's `displayName`, cached until their profile changes.
import '../core/core_service.dart';

/// Display names by user, fetched from the core.
class UserNames {
  UserNames(this.core);

  final CoreService core;
  final Map<String, String> _names = {};

  static String _key(UserRef user) => '${user.username}@${user.serverId}';

  /// The cached name of [user], or its username until [load] got it.
  String of(UserRef user) => _names[_key(user)] ?? user.username;

  /// Fetches the names of [users] not cached yet.
  Future<void> load(Iterable<UserRef> users) async {
    for (final user in users) {
      final key = _key(user);
      if (_names.containsKey(key)) continue;
      _names[key] = await core.displayName(user);
    }
  }

  /// [user]'s profile changed: fetch the name again next time.
  void forget(UserRef user) => _names.remove(_key(user));
}
