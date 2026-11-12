// The app's named routes (flutter-app design D3: Navigator, no routing
// package). Each builder gets the one CoreService.
import 'package:flutter/material.dart';

import '../admin/group_name_screen.dart';
import '../admin/invite_screen.dart';
import '../admin/invites_screen.dart';
import '../admin/remove_user_screen.dart';
import '../core/core_service.dart';
import '../devices/approve_link_screen.dart';
import '../devices/link_this_device_screen.dart';
import '../members/members_screen.dart';
import '../profile/profile_screen.dart';

/// Route names.
abstract final class Routes {
  static const members = '/members';
  static const profile = '/profile';
  static const linkThisDevice = '/devices/link-this';
  static const approveLink = '/devices/approve';
  static const invite = '/admin/invite';
  static const invites = '/admin/invites';
  static const removeUser = '/admin/remove-user';
  static const groupName = '/admin/group-name';
}

/// The route table over [core].
Map<String, WidgetBuilder> appRoutes(CoreService core) => {
  Routes.members: (_) => MembersScreen(core: core),
  Routes.profile: (_) => ProfileScreen(core: core),
  Routes.linkThisDevice: (_) => LinkThisDeviceScreen(core: core),
  Routes.approveLink: (_) => ApproveLinkScreen(core: core),
  Routes.invite: (_) => InviteScreen(core: core),
  Routes.invites: (_) => InvitesScreen(core: core),
  Routes.removeUser: (_) => RemoveUserScreen(core: core),
  Routes.groupName: (_) => GroupNameScreen(core: core),
};
