# Release signing: public fingerprints only

app-release design D3. The keys themselves live with the owner, outside the
repo, never in CI (`~/.vyajena-release/`, passwords in the macOS Keychain, a
backup in the password manager). This file holds only **public** certificate
fingerprints, which `release-check.sh` compares against the bundle's signer.

After making the upload key (docs/release.md, step 2), the owner replaces the
placeholder below with the output of:

    keytool -list -v -keystore ~/.vyajena-release/upload.jks -alias upload | grep SHA256

Upload key SHA-256: <owner: fill in after making the upload key>

Play App Signing key: held by Google. Its SHA-256 (Play Console › Test and
release › App integrity) is not needed by the check.
