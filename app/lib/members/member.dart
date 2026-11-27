// app-ui: Profile and members. The member data model shared across the Contacts
// tab, the member profile and the admin screens: a user and the devices they have
// in the group. (The standalone members list was retired by `shell-nav-consistency`
// — the Contacts tab and the member profile supersede it.)
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
