## 0. Status

- [ ] 0.1 **DEFERRED — design only.** No implementation work is tracked here. The spec
  deltas exist only to keep the design reviewable (`openspec validate
  trust-verification --strict`). Do not start coding from this change.

## 1. Promotion checkpoints (each needs owner approval before it becomes build-ready)

- [ ] 1.1 Part A (recovery phrase + `RecoverDevice`): owner approves; settle Argon2id
  parameters and the recovery-attempt lockout curve (design D1–D4); confirm the MLS
  rejoin + history-loss framing (D5); then split into its own build-ready change
- [ ] 1.2 Part B (quorum re-trust): owner approves; settle the threshold shape across
  group sizes and what admin-trust concretely gates once re-established (design D6–D8);
  then split into its own build-ready change
- [ ] 1.3 Part C (verified badges): owner approves; settle the attestation format,
  storage and revocation, and whether member→member attestations are allowed (design
  D9–D10); then split into its own build-ready change
- [ ] 1.4 Reconcile the shared attestation format with `replace-user` Phase 2 co-signed
  succession so the two do not duplicate signing machinery (design: Relationship)
