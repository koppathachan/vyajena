//! server-deployment (push): the committed deploy files for the egress proxy and
//! the push credentials, checked as written; `deploy.sh`'s credential guards
//! run in temp folders and stop before any SSH. The server itself is checked by
//! `deploy/server/check.sh`.

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use server::push::PushSettings;
use support::Check;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .check("repo root")
        .to_path_buf()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo().join(path)).check(path)
}

fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|l| l == line)
}

// spec: server-deployment/sandboxed-service
#[test]
fn the_server_stays_loopback_only_and_only_the_egress_proxy_reaches_out() {
    let server = read("deploy/server/vyajena.service");
    assert!(has_line(&server, "IPAddressAllow=localhost"));
    assert!(has_line(&server, "IPAddressDeny=any"));
    assert!(has_line(
        &server,
        "ExecStart=/usr/local/bin/vyajena-server --config /etc/vyajena/config.toml --push /etc/vyajena/push.toml"
    ));
    let review = read("deploy/server/vyajena-review.service");
    assert!(!review.contains("--push"), "the review server has no push");
    assert!(has_line(&review, "IPAddressAllow=localhost"));

    let egress = read("deploy/server/vyajena-egress.service");
    for line in [
        "DynamicUser=yes",
        "ExecStart=/usr/local/bin/vyajena-server egress-proxy --listen 127.0.0.1:@EGRESS_PORT@",
        "NoNewPrivileges=yes",
        "ProtectSystem=strict",
        "ProtectHome=yes",
        "PrivateTmp=yes",
        "PrivateDevices=yes",
        "PrivateUsers=yes",
        "CapabilityBoundingSet=",
        "AmbientCapabilities=",
        "RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX",
        "Restart=always",
        "IPAddressAllow=localhost @ROUTER@/32",
        "IPAddressDeny=0.0.0.0/8 10.0.0.0/8 100.64.0.0/10 169.254.0.0/16 172.16.0.0/12 192.168.0.0/16 fc00::/7 fe80::/10",
    ] {
        assert!(
            has_line(&egress, line),
            "vyajena-egress.service lacks {line}"
        );
    }
    assert!(!egress.contains("StateDirectory"));
    assert!(
        !egress.contains("LoadCredential"),
        "the proxy holds no secret"
    );
}

// spec: server-deployment/push-credentials
#[test]
fn push_toml_names_the_real_servers_folder_and_holds_no_secret() {
    let text = render_file("push.toml");
    let settings = PushSettings::from_toml(&text, Path::new("/etc/vyajena")).check("push.toml");
    assert_eq!(settings.credentials_dir, Path::new("/var/lib/vyajena/push"));
    assert_eq!(
        settings.proxy.as_deref(),
        Some(format!("http://127.0.0.1:{}", network("EGRESS_PORT")).as_str())
    );
    assert_eq!(settings.apns_topic, "com.purplechaamba.vyajena");
    for secret in ["PRIVATE KEY", "private_key", "AIza"] {
        assert!(!text.contains(secret));
    }
    // No push key or Firebase file is tracked, and git ignores them.
    let tracked = Command::new("git")
        .args(["ls-files"])
        .current_dir(repo())
        .output()
        .check("git ls-files");
    let tracked = String::from_utf8_lossy(&tracked.stdout);
    for name in ["fcm-service-account.json", "google-services.json", ".p8"] {
        assert!(
            !tracked.lines().any(|l| l.ends_with(name)),
            "{name} is in the repo"
        );
    }
    let ignore = read(".gitignore");
    for pattern in [
        "google-services.json",
        "fcm-service-account.json",
        "AuthKey_*.p8",
    ] {
        assert!(has_line(&ignore, pattern), ".gitignore lacks {pattern}");
    }
}

// spec: server-deployment/push-credentials
#[test]
fn deploy_copies_the_keys_owner_only_and_never_prints_them() {
    let deploy = read("deploy/server/deploy.sh");
    for line in [
        "PUSH_DIR=\"${PUSH_DIR:-$HOME/.vyajena-push}\"",
        "    server \"umask 077; cat > '$staging/push/${key##*/}'\" <\"$key\"",
        "  install -d -m 0700 -o vyajena -g vyajena \"$dir\"",
        "    install -m 0600 -o vyajena -g vyajena \"$f\" \"$dir/${f##*/}\"",
        "  local staged=\"$1\" dir=/var/lib/vyajena/push f name",
        "  install -m 0644 -o root -g root \"$staging/push.toml\" /etc/vyajena/push.toml",
        "  systemctl restart vyajena-egress.service",
    ] {
        assert!(has_line(&deploy, line), "deploy.sh lacks {line}");
    }
    assert!(!deploy.contains("vyajena-review/push"));
    // Rollback to a binary from before push drops --push.
    assert!(deploy.contains(
        "sed -i 's| --push /etc/vyajena/push.toml||' /etc/systemd/system/vyajena.service"
    ));
}

/// A value of the example network settings (the real ones are outside the
/// repository, change generic-deployment).
fn network(key: &str) -> String {
    read("deploy/network.conf.example")
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .map(str::to_owned)
        .check(key)
}

/// A folder with the example `network.conf`, as `RELAY_SECRETS`.
fn network_secrets() -> tempfile::TempDir {
    let dir = tempfile::tempdir().check("secrets");
    std::fs::write(
        dir.path().join("network.conf"),
        read("deploy/network.conf.example"),
    )
    .check("network.conf");
    dir
}

/// A host-side template with the example network settings filled in.
fn render_file(name: &str) -> String {
    let secrets = network_secrets();
    let out = Command::new("bash")
        .arg(repo().join("deploy/relay/render.sh"))
        .args(["server-file", name])
        .env("RELAY_SECRETS", secrets.path())
        .env("HOME", secrets.path())
        .output()
        .check("render.sh");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).check("utf-8")
}

/// Runs `deploy.sh --no-build` with `PUSH_DIR=dir`; it must stop before SSH.
fn deploy_with(dir: &Path) -> (bool, String) {
    let backups = tempfile::tempdir().check("backups");
    let secrets = network_secrets();
    let out = Command::new("bash")
        .arg(repo().join("deploy/server/deploy.sh"))
        .arg("--no-build")
        .env("PUSH_DIR", dir)
        .env("BACKUP_ROOT", backups.path())
        .env("RELAY_SECRETS", secrets.path())
        .env("SERVER", "nobody@192.0.2.1")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .check("deploy.sh");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).check("chmod");
}

// spec: server-deployment/push-credentials
#[test]
fn deploy_refuses_push_keys_open_to_others() {
    let home = tempfile::tempdir().check("temp");
    let dir = home.path().join(".vyajena-push");
    std::fs::create_dir(&dir).check("mkdir");

    // "Folder open to others".
    chmod(&dir, 0o755);
    let (ok, err) = deploy_with(&dir);
    assert!(!ok);
    assert!(
        err.contains(&format!("{} has mode 755", dir.display())),
        "{err}"
    );

    chmod(&dir, 0o700);
    let key = dir.join("fcm-service-account.json");
    std::fs::write(&key, "{}").check("key");
    chmod(&key, 0o644);
    let (ok, err) = deploy_with(&dir);
    assert!(!ok);
    assert!(
        err.contains("fcm-service-account.json has mode 644"),
        "{err}"
    );
    chmod(&key, 0o600);

    for name in ["AuthKey_ABC123DEFG.p8", "AuthKey_ZZZ999YYYX.p8"] {
        let p8 = dir.join(name);
        std::fs::write(&p8, "x").check("p8");
        chmod(&p8, 0o600);
    }
    let (ok, err) = deploy_with(&dir);
    assert!(!ok);
    assert!(err.contains("has 2 AuthKey_*.p8 files"), "{err}");
    std::fs::remove_file(dir.join("AuthKey_ZZZ999YYYX.p8")).check("rm");
    std::fs::rename(
        dir.join("AuthKey_ABC123DEFG.p8"),
        dir.join("AuthKey_short.p8"),
    )
    .check("mv");
    let (ok, err) = deploy_with(&dir);
    assert!(!ok);
    assert!(
        err.contains("name it AuthKey_<10-character key id>.p8"),
        "{err}"
    );
}

// spec: server-deployment/hardening-check
#[test]
fn the_server_check_covers_the_egress_proxy_and_the_push_keys() {
    let check = read("deploy/server/check.sh");
    for needle in [
        "egress-running",
        "egress sandbox $prop",
        "exposure_of vyajena-egress.service \"egress sandbox\"",
        "egress refuses example.com:443",
        "egress reaches $target",
        "file-mode $pdir",
        "warn push-fcm",
        "warn push-apns",
        "review no push",
        "egress listening on loopback",
        "for port in \"$SERVER_PORT\" \"$REVIEW_SERVER_PORT\" \"$EGRESS_PORT\"; do",
    ] {
        assert!(check.contains(needle), "check.sh lacks {needle}");
    }
}
