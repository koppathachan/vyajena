> **DEFERRED — DESIGN ONLY. Do not implement from this change yet.**
> This change captures the full trust-recovery design so the thinking is not lost. It
> is the richer successor to the bare key-change acknowledgement shipped in
> `admin-recovery`. Nothing here is build-ready: the requirements below are a design
> record (they validate `--strict` so they stay reviewable), not a task queue. Promote
> parts to their own build-ready changes only on owner approval.

## Why

`admin-recovery` unbreaks a locked-out admin with an operator reset, but it pays for it
two ways: the recovered admin gets a **new** identity key (so every peer must re-trust
by acknowledging a changed safety number), and the only trust handling is a bare
per-member warning. That is the correct **minimum**, but it is coarse:

- Re-keying is avoidable. If the admin could recover the **same** identity key, no peer
  would have to re-trust anything.
- A bare "key changed — accept?" banner gives each member no evidence beyond the
  server's word. A hostile or coerced server that swaps in an attacker's key still
  gets a yes/no prompt with nothing to weigh.
- There is no notion of a **verified** identity at all, so members cannot tell a
  vouched key from an unvouched one.

This change designs the web-of-trust that fixes all three, for the **single-admin**
self-hosted deployment. It is distinct from `replace-user` Phase 2 co-signed
succession, which is the **multi-admin** variant (several admins co-sign a successor);
here there is one admin plus a web of signed member attestations.

## What Changes (all deferred)

Three parts, each promotable on its own.

### Part A — Recovery phrase (same-key recovery)

A fresh install can recover the user's **existing** identity key from a user-chosen
recovery phrase, so recovery needs **no re-trust** (the pinned key is unchanged).

- The admin (or any user) **chooses** the phrase — it is user-picked, not
  app-generated — minimum **24 words**.
- **Caveat (must spec):** the phrase is **NOT the seed**. A user-chosen phrase is
  low-entropy; using it directly as the key would weaken the identity key to the
  phrase's guessability. Instead the identity key is a **full-entropy random** key,
  stored as an **encrypted blob** wrapped by a key **derived from the phrase with
  Argon2id**. Recovery unwraps the blob; it does not derive the key from the phrase.
- A **passphrase strength check** gates phrase choice: word count alone is not
  security (24 copies of "password" is 24 words). Estimate entropy, reject weak
  phrases.
- **Rate-limited / locked** recovery attempts: unwrap is offline-guessable, so cap
  attempts and back off / lock after repeated failures.
- Recovery flow: fresh install → enter phrase → Argon2id → unwrap the identity key →
  it **matches the already-pinned key** → a **new `RecoverDevice` RPC** admits a device
  certificate signed by that pinned identity key **without an existing device
  approving** (it is `LinkDevice` minus the approve step; the identity-key signature is
  the authority). Same identity ⇒ **no re-trust** for any peer.
- **Open wrinkle (spec it):** MLS group state is not recovered by the phrase. The new
  leaf must be **re-added** to the lobby and groups (a fresh Add/Welcome), and
  **on-device encrypted history is unrecoverable regardless** (max_past_epochs = 0,
  keys never leave the core). The phrase recovers identity, not history.

### Part B — Quorum re-trust (nicer break-glass re-key path)

For the case where the admin *did* re-key (used `admin-recovery`, had no phrase), a
better path than the bare per-member warning: the admin re-establishes trust by an
**in-band challenge** that a **quorum** of members confirm.

- The admin poses an in-band challenge to members — a shared-secret question, or a
  true/false only the two of them know — and a **quorum** (e.g. 3 of 5 members)
  confirming re-establishes admin trust.
- **Caveat (must spec):** each confirming member's vouch is **cryptographically signed
  by that member's identity key**, so a hostile or lying server cannot forge or inflate
  the tally. **Signed attestations are the trust primitive.**
- The quorum is **advisory / surfaced**, never silent: a receiving device shows "3 of 5
  vouched — accept?" **and WHO vouched**, and still requires the **owner's tap**. It
  SHALL NEVER silently auto-accept a key onto the other members on the strength of the
  tally.
- Handle **tiny and variable membership** and **offline members**: the threshold is a
  floor/fraction with sane small-group behaviour (a 2-person group cannot need 3
  vouches), and offline members simply have not vouched yet (the tally is a snapshot,
  not a vote that closes).
- Owner prefers this over pure out-of-band QR comparison.

### Part C — Verified badges

A visible notion of a verified identity, built from the same signed attestations.

- The **admin has an intrinsic badge**: the admin owns the server, and the
  bootstrap/invite chain already proves which user id is the admin — no attestation
  needed.
- **Members have no badge by default.** A member may **request** a badge from the admin
  (out-of-band, or via the Part B challenge method). The admin **grants** it by
  **signing an attestation** binding "member X's key = K".
- A **badge is a valid admin signature** over that binding; revoking it is the admin
  signing a revocation (or the key changing). **All trust is signed attestations** —
  the badge is just the admin's attestation surfaced.

## Open decisions (for when a part is promoted)

- Argon2id parameters and the recovery-attempt lockout curve (Part A).
- Quorum threshold shape across group sizes; what "the admin trust" concretely gates
  once re-established (Part B).
- Attestation format, storage and revocation; whether member→member attestations are
  allowed or only admin→member (Part C).
- How Parts B/C attestations relate to (and avoid duplicating) `replace-user` Phase 2
  co-signed succession.

## Proto files touched (at implementation, not now)

- `proto/vyajena/v1/devices.proto` (or equivalent): a new `RecoverDevice` RPC for Part
  A (LinkDevice minus the approve step; authority = identity-key signature).
- A signed-attestation message for Parts B and C (member vouch; admin badge grant /
  revoke). Field numbers and placement are deferred.

## Invariants

No invariant is changed by this design record. If promoted, Part A adds a path that
admits a device on an **identity-key signature** rather than an existing device's
approval — this must be specced as consistent with "verify each leaf's device
certificate against the user's pinned identity key" (the signature IS that authority),
and must not weaken `device-identity: Device linking`. Parts B and C add signed
attestations as first-class trust data; they never let the server assert trust on its
own say-so.
