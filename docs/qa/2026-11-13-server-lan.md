# QA run: the app through the real server on the home LAN (2026-11-13)

Manual run during `server-deployment` (archived change, tasks 3.2–3.3), recorded
from its screenshots on 2026-11-14.

**Setup**

```
iOS Simulator iPhone 16 (admin akhil@chat) ─┐
                                          ├─▶ server <server-address>:<server-port> (vyajena.service)
Android emulator vyajena_test (bob@chat) ───┘    no adb reverse
```

- App: `com.purplechaamba.vyajena`, debug build. server: release build, sandboxed service.
- Only the Android (Bob) side was captured as screenshots; the admin side is
  recorded in the session's notes.

## Results

| # | Step | Spec | Result |
| - | ---- | ---- | ------ |
| 1 | Fresh install opens on "Join the group" | app-ui: Join from an invite | ✅ |
| 2 | Paste Bob's invite, Check invite → shows `bob@chat` on `<server-address>:<server-port>` before joining | app-ui: Join from an invite | ✅ |
| 3 | Join → chat opens with no admin tap (auto-add), connected icon | group-membership: Automatic add of new users | ✅ |
| 4 | Bob sends "hi from bob" → ✓ accepted | app-ui: Chat screen | ✅ |
| 5 | Admin's reply "hey bob, from the server" arrives | app-ui: Chat screen | ✅ |
| 6 | server upgraded while both apps connected → Bob sends "after upgrade" → ✓, no re-join | server-deployment: Install and upgrade | ✅ |
| 7 | server rolled back → admin's "after rol…" arrives | server-deployment: Install and upgrade | ✅ (text cut by the simulator's typing, not the app) |
| 8 | server journal holds no message text | app-ui: UI goes through the core | ✅ |
| 9 | Kill -9 on the server → back in 2 s, same pin | server-deployment: Sandboxed service | ✅ (no screenshot) |

## Screenshots (Bob, Android)

| 1 Join | 2 Invite checked | 3 Auto-added |
| --- | --- | --- |
| ![join](img/2026-11-13-01-join-screen.png) | ![invite](img/2026-11-13-02-invite-checked.png) | ![added](img/2026-11-13-03-auto-added.png) |

| 4 Sent ✓ | 5 Reply | 6 After upgrade | 7 After rollback |
| --- | --- | --- | --- |
| ![sent](img/2026-11-13-04-sent-accepted.png) | ![reply](img/2026-11-13-05-reply-received.png) | ![upgrade](img/2026-11-13-06-after-upgrade.png) | ![rollback](img/2026-11-13-07-after-rollback.png) |

The invite in screenshot 2 was single-use and is already used.

## Findings from the screenshots

- **Own messages look odd:** they stretch the full width, with a tall empty
  row under the text for the ✓, and are left-aligned like everyone else's.
  Expected: own messages right-aligned and compact.
- **Own messages are labelled "bob"** on Bob's own phone. A "You" label, or none,
  would read better.
- No display names were set, so senders show as usernames ("akhil", "bob");
  the "Ali (alice)" form was not exercised.

## Not tested yet

Most of these were run on 2026-11-14: see [2026-11-14-server-lan-run2.md](2026-11-14-server-lan-run2.md).

| Area | What |
| --- | --- |
| Real phones | A physical iPhone and Android phone on the home Wi-Fi |
| Admin screens | List and revoke invites, remove a user, rename the group |
| Profiles | Set a display name and see "Ali (alice)" everywhere |
| Members | Members screen with device counts |
| Devices | Link a second device by text (request → approve → join) |
| Reset user | Remove Bob, re-invite, key-changed warning, admin accepts, re-add |
| Offline | Airplane mode: send while offline (⏳), back online (✓), catch-up order |
| Background | App in background for an hour, back to foreground, reconnect |
| Limits | Very long paste refused in the composer |
| Look | Dark mode |
| Server power | Pull the server's power mid-chat; nothing accepted is lost |
| Server updates | Full upgrade + reboot; apps reconnect, same pin |
| Inactivity | Removed after 7 days offline, rejoin notice (covered by automated tests only) |

## IPv6 router test (2026-11-14)

With the server's own IPv6 block lifted for the test (temporary nftables rules
allowing inbound IPv6 TCP 22, <server-port> and ping, confirmed live; no reboot), the
owner ran the outside check from a phone hotspot on mobile data: **no ping
answer, and ports 22 and <server-port> closed or filtered on both of the server's global IPv6
addresses**. sshd listens on IPv6, so a router that passed inbound IPv6 would
have shown port 22 open: **the Airtel router drops inbound IPv6 on its own**.
The server's firewall was restored right after. Reaching the server directly over IPv6
(instead of through a relay) is left as a possible later improvement (owner).
