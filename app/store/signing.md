# Release signing: public fingerprints only

app-release design D3. The keys themselves live with the owner, outside the
repo, never in CI (`~/.vyajena-release/`, passwords in the macOS Keychain, a
backup in the password manager). This file holds only **public** certificate
fingerprints, which `release-check.sh` compares against the bundle's signer.

Recorded 2026-11-15 (CN=Akhil Sasidharan, O=purplechaamba.com, C=IN). After
a new upload key (or a Play upload key reset), replace it with the output of:

    keytool -list -v -keystore ~/.vyajena-release/upload.jks -alias upload | grep SHA256

Upload key SHA-256: 64:EE:5E:F6:92:76:BA:18:D7:A8:E0:BB:25:7D:44:84:7A:FC:C4:42:13:96:3C:32:55:1B:EA:76:F1:BA:4F:38

Play App Signing key: held by Google. Its SHA-256 (Play Console › Test and
release › App integrity) is not needed by the check.
