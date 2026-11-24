# Tasks

App-only (Flutter). No proto, core, or server change. Tick a task only when its
tests pass.

## 1. Auto-create the lobby (app-ui: The lobby appears without a manual create step)

- [ ] 1.1 `app/lib/chat/group_gate.dart`: when `admin && !inGroup`, call
  `core.createGroup()` automatically instead of showing the "Create the group"
  button; show a brief setting-up state while it is in flight.
- [ ] 1.2 Handle `ALREADY_EXISTS` from `Admin.CreateGroup` as benign (a linked
  admin device): no error, wait to be synced into the existing lobby (the gate's
  existing `JoinedGroup`/`MembersChanged`/`Rejoined` listener brings it in).
- [ ] 1.3 On any other error, show the error with a retry (reuse the old
  create-screen error affordance) so the admin is never stuck on a silent failure.
- [ ] 1.4 Keep the manual named-group creation path (`createNamedGroup`) unchanged.

## 2. Tests

- [ ] 2.1 Widget tests (`app/test/`, `// spec: app-ui/the-lobby-appears-without-a-
  manual-create-step`): the admin's first device calls `createGroup` with no tap
  and shows the lobby; an `ALREADY_EXISTS` result shows no error and resolves to the
  lobby on sync; a non-admin device never calls `createGroup`.

## 3. Validate / verify

- [ ] 3.1 `cd app && flutter analyze && flutter test` clean.
- [ ] 3.2 `openspec validate auto-lobby --strict` passes.
- [ ] 3.3 Simulator: a fresh server + admin device lands straight in the lobby with
  no "Create the group" tap (coordinate with the live test session; serialise
  builds).
