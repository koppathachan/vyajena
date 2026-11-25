-- admin-recovery / key-directory: Clients verify the identity chain. When a
-- pinned identity key is replaced automatically to keep the group decrypting
-- (a reset user or the recovered admin re-keys), the new key is pinned but the
-- identity is marked unverified / pending re-trust until the device owner
-- acknowledges it. `retrust_since` holds when that re-trust was first needed;
-- NULL means the pinned key is trusted (first sight, or explicitly accepted or
-- acknowledged).
ALTER TABLE identity_keys ADD COLUMN retrust_since INTEGER;
