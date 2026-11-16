//! relay and pi-deployment: the committed deploy files (relay design D11).
//! Checks `deploy/relay/` and `deploy/pi/` as written, and runs the scripts
//! that need no host (`secrets.sh`, `render.sh`, `create.sh --dry-run`,
//! `deploy.sh --reset` up to its guards) in temp folders, with fake `ssh`,
//! `scp` and `gcloud` that only record being called. The hosts themselves are
//! checked by `deploy/relay/check.sh` and `deploy/pi/check.sh`.

mod support;

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use server::runtime::Config;
use support::Check;

const RELAY_IP: &str = "203.0.113.10";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .check("repo root")
        .to_path_buf()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo().join(path)).check(path)
}

/// `deploy/relay/relay.conf` as a map.
fn relay_conf() -> BTreeMap<String, String> {
    read("deploy/relay/relay.conf")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (key, value) = line.split_once('=').check("KEY=value");
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

fn conf(key: &str) -> String {
    relay_conf().get(key).cloned().check(key)
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .check("metadata")
        .permissions()
        .mode()
        & 0o777
}

/// A temp home with fake `ssh`, `scp` and `gcloud` first on PATH; each
/// appends its arguments to `calls.log` and fails.
struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().check("temp dir");
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).check("bin");
        for tool in ["ssh", "scp", "gcloud"] {
            let path = bin.join(tool);
            let log = dir.path().join("calls.log");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho \"{tool} $*\" >> '{}'\nexit 1\n",
                    log.display()
                ),
            )
            .check("fake tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).check("chmod");
        }
        Self { dir }
    }

    fn secrets(&self) -> PathBuf {
        self.dir.path().join("relay-secrets")
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("calls.log")).unwrap_or_default()
    }

    fn run(&self, script: &str, args: &[&str], stdin: &str) -> Output {
        let path = format!(
            "{}:{}",
            self.dir.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new("bash")
            .arg(repo().join(script))
            .args(args)
            .env("PATH", path)
            .env("HOME", self.dir.path())
            .env("RELAY_SECRETS", self.secrets())
            .env("BACKUP_ROOT", self.dir.path().join("backups"))
            .env("PI", "nobody@192.0.2.1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .check(script);
        if let Some(mut input) = child.stdin.take() {
            use std::io::Write as _;
            input.write_all(stdin.as_bytes()).check("stdin");
        }
        child.wait_with_output().check(script)
    }

    /// Secrets made by `secrets.sh`, plus the relay.env `create.sh` writes.
    fn with_secrets(self) -> Self {
        let out = self.run("deploy/relay/secrets.sh", &[], "");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::write(
            self.secrets().join("relay.env"),
            format!("RELAY_IP={RELAY_IP}\n"),
        )
        .check("relay.env");
        self
    }

    fn secret(&self, name: &str) -> String {
        std::fs::read_to_string(self.secrets().join(name))
            .check(name)
            .trim()
            .to_owned()
    }

    fn render(&self, what: &str) -> String {
        let out = self.run("deploy/relay/render.sh", &[what], "");
        assert!(
            out.status.success(),
            "render {what}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).check("utf-8")
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn toml(text: &str) -> toml::Table {
    text.parse().check("rendered TOML")
}

fn get<'a>(table: &'a toml::Table, path: &str) -> &'a toml::Value {
    let mut parts = path.split('.');
    let first = parts.next().check("path");
    let mut value = table.get(first).check(path);
    for part in parts {
        value = value.get(part).check(path);
    }
    value
}

fn string(table: &toml::Table, path: &str) -> String {
    get(table, path).as_str().check(path).to_owned()
}

/// The `create.sh --dry-run` commands, one per line.
fn dry_run(sandbox: &Sandbox) -> Vec<String> {
    let out = sandbox.run("deploy/relay/create.sh", &["--dry-run"], "");
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8(out.stdout)
        .check("utf-8")
        .lines()
        .map(|line| line.replace('\\', ""))
        .collect()
}

// --- relay ---------------------------------------------------------------------

// spec: relay/relay-host
#[test]
fn the_relay_is_a_free_tier_e2_micro_in_the_us_on_its_own_network() {
    let region = conf("REGION");
    assert!(
        ["us-central1", "us-east1", "us-west1"].contains(&region.as_str()),
        "{region} is not a free-tier region"
    );
    assert!(conf("ZONE").starts_with(&format!("{region}-")));
    assert_eq!(conf("MACHINE_TYPE"), "e2-micro");
    assert_eq!(conf("DISK_TYPE"), "pd-standard");
    let disk: u32 = conf("DISK_GB").parse().check("DISK_GB");
    assert!(disk <= 30, "{disk} GB is over the free 30 GB");
    assert_eq!(conf("PROJECT"), "vyajena");
    assert_eq!(conf("PUBLIC_PORT"), "443");
    assert_eq!(conf("RELAY_HOST"), "vyajarelay.purplechaamba.com");

    let sandbox = Sandbox::new().with_secrets();
    let commands = dry_run(&sandbox);
    let vm = commands
        .iter()
        .find(|c| c.starts_with("gcloud compute instances create"))
        .check("instance create");
    for flag in [
        "--machine-type=e2-micro",
        "--no-service-account",
        "--no-scopes",
        "--boot-disk-type=pd-standard",
        "subnet=vyajena-relay,address=",
        "block-project-ssh-keys=TRUE",
        "enable-oslogin=FALSE",
        "serial-port-enable=FALSE",
    ] {
        assert!(vm.contains(flag), "VM create lacks {flag}: {vm}");
    }
    // Its own VPC, never `default`, and a static address (no DNS).
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("gcloud compute networks create vyajena-relay"))
    );
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("gcloud compute addresses create vyajena-relay"))
    );
    assert!(
        !commands
            .iter()
            .any(|c| c.contains("network=default") || c.contains("dns"))
    );
}

// spec: relay/relay-firewall
#[test]
fn the_firewall_opens_443_and_the_tunnel_and_ssh_only_from_iap() {
    let sandbox = Sandbox::new().with_secrets();
    let commands = dry_run(&sandbox);
    let rules: Vec<&String> = commands
        .iter()
        .filter(|c| c.contains("firewall-rules create"))
        .collect();
    assert_eq!(rules.len(), 3, "{rules:#?}");
    let tunnel = conf("TUNNEL_PORT");
    let expect = [
        ("tcp:443", "0.0.0.0/0"),
        (&*format!("tcp:{tunnel}"), "0.0.0.0/0"),
        ("tcp:22", "35.235.240.0/20"),
    ];
    for (port, range) in expect {
        let rule = rules
            .iter()
            .find(|r| r.contains(&format!("--rules={port} ")))
            .check(port);
        assert!(
            rule.contains(&format!("--source-ranges={range} ")),
            "{rule}"
        );
        assert!(
            rule.contains("--direction=INGRESS --action=ALLOW"),
            "{rule}"
        );
        assert!(rule.contains("--network=vyajena-relay"), "{rule}");
        assert!(rule.contains("--no-enable-logging"), "{rule}");
    }
    let subnet = commands
        .iter()
        .find(|c| c.contains("subnets create"))
        .check("subnet");
    assert!(subnet.contains("--no-enable-flow-logs"), "{subnet}");
}

// spec: relay/tunnel-authentication
#[test]
fn the_tunnel_uses_noise_with_the_pinned_relay_key_and_the_token() {
    let sandbox = Sandbox::new().with_secrets();
    let server = toml(&sandbox.render("relay-server"));
    let client = toml(&sandbox.render("pi-tunnel"));
    let tunnel = conf("TUNNEL_PORT");

    for (side, table) in [("server", &server), ("client", &client)] {
        assert_eq!(string(table, &format!("{side}.transport.type")), "noise");
        assert_eq!(
            string(table, &format!("{side}.transport.noise.pattern")),
            "Noise_NK_25519_ChaChaPoly_BLAKE2s"
        );
        assert_eq!(
            string(table, &format!("{side}.services.vyajena.token")),
            sandbox.secret("token")
        );
        assert_eq!(
            string(table, &format!("{side}.services.vyajena.type")),
            "tcp"
        );
    }
    // The relay holds the private key; the Pi pins the public one.
    assert_eq!(
        string(&server, "server.transport.noise.local_private_key"),
        sandbox.secret("noise.key")
    );
    assert_eq!(
        string(&client, "client.transport.noise.remote_public_key"),
        sandbox.secret("noise.pub")
    );
    assert_ne!(sandbox.secret("noise.key"), sandbox.secret("noise.pub"));
    // The Pi dials out; the relay listens.
    assert_eq!(
        string(&server, "server.bind_addr"),
        format!("0.0.0.0:{tunnel}")
    );
    assert_eq!(
        string(&client, "client.remote_addr"),
        format!("{RELAY_IP}:{tunnel}")
    );
    assert_eq!(
        string(&server, "server.services.vyajena.bind_addr"),
        "0.0.0.0:443"
    );
    assert_eq!(
        string(&client, "client.services.vyajena.local_addr"),
        "127.0.0.1:8443"
    );
}

// spec: relay/relay-host-hardening
#[test]
fn rathole_on_the_relay_is_sandboxed_with_only_the_bind_capability() {
    let unit = read("deploy/relay/vyajena-relay.service");
    for line in [
        "DynamicUser=yes",
        "CapabilityBoundingSet=CAP_NET_BIND_SERVICE",
        "AmbientCapabilities=CAP_NET_BIND_SERVICE",
        "NoNewPrivileges=yes",
        "ProtectSystem=strict",
        "ProtectHome=yes",
        "PrivateTmp=yes",
        "RestrictAddressFamilies=AF_INET AF_INET6",
        "SystemCallFilter=~@privileged @resources",
    ] {
        assert!(
            unit.lines().any(|l| l == line),
            "vyajena-relay.service lacks {line}"
        );
    }
    assert!(
        !unit.contains("PrivateUsers=yes"),
        "PrivateUsers voids the capability"
    );
    let deploy = read("deploy/relay/deploy.sh");
    for line in [
        "PermitRootLogin no",
        "PasswordAuthentication no",
        "KbdInteractiveAuthentication no",
        "Unattended-Upgrade::Automatic-Reboot \"false\";",
    ] {
        assert!(deploy.contains(line), "relay deploy.sh lacks {line}");
    }
    // SSH only through IAP: the scripts dial through start-iap-tunnel.
    let lib = read("deploy/relay/lib.sh");
    assert!(lib.contains("ProxyCommand=gcloud compute start-iap-tunnel"));
    assert!(lib.contains("StrictHostKeyChecking=yes"));
}

// spec: relay/relay-keeps-no-state
#[test]
fn the_relay_has_no_writable_path_and_logs_in_ram_only() {
    let unit = read("deploy/relay/vyajena-relay.service");
    for absent in [
        "StateDirectory",
        "LogsDirectory",
        "CacheDirectory",
        "ReadWritePaths",
    ] {
        assert!(!unit.contains(absent), "vyajena-relay.service has {absent}");
    }
    assert!(unit.lines().any(|l| l == "Environment=RUST_LOG=warn"));
    assert!(unit.contains("LoadCredential=server.toml:/etc/vyajena-relay/server.toml"));
    let deploy = read("deploy/relay/deploy.sh");
    assert!(deploy.contains("Storage=volatile"));
    let sandbox = Sandbox::new().with_secrets();
    let vm = dry_run(&sandbox)
        .into_iter()
        .find(|c| c.starts_with("gcloud compute instances create"))
        .check("instance create");
    assert!(vm.contains("--no-service-account --no-scopes"), "{vm}");
}

// spec: relay/relay-secrets
#[test]
fn secrets_are_made_once_owner_only_and_never_printed() {
    let sandbox = Sandbox::new();
    let first = sandbox.run("deploy/relay/secrets.sh", &[], "");
    assert!(first.status.success(), "{}", text(&first));
    let dir = sandbox.secrets();
    assert_eq!(mode(&dir), 0o700);
    let names = [
        "token",
        "noise.key",
        "noise.pub",
        "ssh_ed25519",
        "ssh_ed25519.pub",
    ];
    let mut values = Vec::new();
    for name in names {
        let path = dir.join(name);
        assert_eq!(mode(&path), 0o600, "{name}");
        values.push(std::fs::read(&path).check(name));
    }
    let token = sandbox.secret("token");
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    for key in ["noise.key", "noise.pub"] {
        assert_eq!(sandbox.secret(key).len(), 44, "{key}: base64 of 32 bytes");
    }
    let shown = text(&first);
    for secret in [
        token.as_str(),
        &sandbox.secret("noise.key"),
        &sandbox.secret("noise.pub"),
    ] {
        assert!(!shown.contains(secret), "secrets.sh printed a secret");
    }

    let second = sandbox.run("deploy/relay/secrets.sh", &[], "");
    assert!(second.status.success());
    assert!(
        text(&second).contains("nothing changed"),
        "{}",
        text(&second)
    );
    for (name, before) in names.iter().zip(values) {
        assert_eq!(
            std::fs::read(dir.join(name)).check(name),
            before,
            "{name} changed"
        );
    }

    // Never inside the repository.
    let inside = Command::new("bash")
        .arg(repo().join("deploy/relay/secrets.sh"))
        .env("RELAY_SECRETS", repo().join("target/should-not-exist"))
        .output()
        .check("secrets.sh in the repo");
    assert!(!inside.status.success());
    assert!(
        text(&inside).contains("inside the repository"),
        "{}",
        text(&inside)
    );
    let _ = std::fs::remove_dir(repo().join("target/should-not-exist"));
}

// spec: relay/relay-secrets
#[test]
fn templates_in_the_repo_hold_placeholders_not_secrets() {
    for path in [
        "deploy/relay/server.toml",
        "deploy/pi/tunnel.toml",
        "deploy/pi/config.toml",
        "deploy/pi/vyajena-tunnel.service",
    ] {
        let text = read(path);
        assert!(text.contains('@'), "{path} has no placeholder");
        for key in ["token", "local_private_key", "remote_public_key"] {
            for line in text.lines().filter(|l| l.trim_start().starts_with(key)) {
                assert!(
                    line.contains(" = \"@"),
                    "{path}: {line} is not a placeholder"
                );
            }
        }
    }
    // Rendering refuses a secret in the wrong format instead of writing it.
    let sandbox = Sandbox::new().with_secrets();
    std::fs::write(sandbox.secrets().join("token"), "not hex\n").check("token");
    let out = sandbox.run("deploy/relay/render.sh", &["relay-server"], "");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

// spec: relay/provision-and-deploy-the-relay
#[test]
fn the_dry_run_prints_every_gcloud_command_and_runs_none() {
    let sandbox = Sandbox::new().with_secrets();
    let commands = dry_run(&sandbox);
    assert!(
        sandbox.calls().is_empty(),
        "dry run called: {}",
        sandbox.calls()
    );
    let gcloud: Vec<&String> = commands
        .iter()
        .filter(|c| c.starts_with("gcloud "))
        .collect();
    // APIs, network, subnet, 3 rules, address, VM, host keys.
    assert_eq!(gcloud.len(), 9, "{gcloud:#?}");
    assert!(gcloud.iter().all(|c| c.contains("--project=vyajena")));
    // The real run refuses without a gcloud login (the fake fails).
    let out = sandbox.run("deploy/relay/create.sh", &[], "");
    assert!(!out.status.success());
    assert!(text(&out).contains("gcloud auth login"), "{}", text(&out));
    assert!(!sandbox.calls().contains("create"), "{}", sandbox.calls());

    // rathole: the pinned version from crates.io, with its own lock file.
    let build = read("deploy/relay/build.sh");
    assert!(build.contains("cargo install \"rathole@$RATHOLE_VERSION\" --locked"));
    assert_eq!(conf("RATHOLE_VERSION"), "0.5.0");
    let deploy = read("deploy/relay/deploy.sh");
    assert!(deploy.contains("--rollback"));
    assert!(deploy.contains("\"$bin.prev\""));
}

// spec: relay/relay-hardening-check
#[test]
fn the_relay_check_names_every_area() {
    let check = read("deploy/relay/check.sh");
    for name in [
        "vm type",
        "vm region",
        "disk",
        "vm service account",
        "static address",
        "dns",
        "firewall rule",
        "flow logs",
        "ssh root-login",
        "sandbox exposure",
        "rathole version",
        "listening",
        "unattended-upgrades no-reboot",
        "journal volatile",
        "warn reboot-required",
        "warn tunnel",
    ] {
        assert!(check.contains(name), "relay check.sh has no {name:?} check");
    }
}

// --- pi-deployment ----------------------------------------------------------

// spec: pi-deployment/reachable-through-the-relay
#[test]
fn the_pi_listens_on_loopback_and_invites_name_the_relay() {
    let sandbox = Sandbox::new().with_secrets();
    let config = Config::from_toml(&sandbox.render("pi-config"), Path::new("/"))
        .check("rendered deploy/pi/config.toml");
    assert_eq!(config.listen_address.to_string(), "127.0.0.1:8443");
    // Invites name the relay's DNS name; the Pi itself never resolves it.
    assert_eq!(config.public_address, "vyajarelay.purplechaamba.com:443");
    assert!(!sandbox.render("pi-config").contains(RELAY_IP));
    assert_eq!(config.admin_user_id(), "vyajan@purplechaamba.com");
}

// spec: pi-deployment/host-hardening
#[test]
fn nftables_opens_only_ssh_from_home() {
    let rules = read("deploy/pi/nftables.conf");
    assert!(
        !rules.contains("8443"),
        "the server port is still in nftables.conf"
    );
    let accepts: Vec<&str> = rules
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("tcp dport"))
        .collect();
    assert_eq!(accepts, ["ip saddr $home_v4 tcp dport 22 accept"]);
    assert!(rules.contains("type filter hook input priority filter; policy drop;"));
}

// spec: pi-deployment/sandboxed-service
#[test]
fn the_server_reaches_loopback_only_and_the_tunnel_only_the_relay() {
    let server = read("deploy/pi/vyajena.service");
    assert!(server.lines().any(|l| l == "IPAddressAllow=localhost"));
    assert!(server.lines().any(|l| l == "IPAddressDeny=any"));

    let sandbox = Sandbox::new().with_secrets();
    let tunnel = sandbox.render("pi-tunnel-unit");
    for line in [
        "DynamicUser=yes".to_owned(),
        "LoadCredential=tunnel.toml:/etc/vyajena/tunnel.toml".to_owned(),
        "NoNewPrivileges=yes".to_owned(),
        "ProtectSystem=strict".to_owned(),
        "ProtectHome=yes".to_owned(),
        "PrivateTmp=yes".to_owned(),
        "PrivateUsers=yes".to_owned(),
        "CapabilityBoundingSet=".to_owned(),
        "AmbientCapabilities=".to_owned(),
        "RestrictAddressFamilies=AF_INET AF_INET6".to_owned(),
        "Restart=always".to_owned(),
        format!("IPAddressAllow=localhost {RELAY_IP}"),
        "IPAddressDeny=any".to_owned(),
    ] {
        assert!(
            tunnel.lines().any(|l| l == line),
            "vyajena-tunnel.service lacks {line}"
        );
    }
    assert!(!tunnel.contains("StateDirectory"));
}

// spec: pi-deployment/build-for-the-pi
#[test]
fn the_pi_build_also_builds_rathole_for_arm64() {
    let build = read("deploy/pi/build.sh");
    assert!(build.contains("\"$repo/deploy/relay/build.sh\" arm64"));
    let relay_build = read("deploy/relay/build.sh");
    assert!(relay_build.contains("arm64) arch=arm64 out=\"$repo/target/pi/rathole\""));
}

// spec: pi-deployment/install-and-upgrade
#[test]
fn deploy_installs_the_tunnel_with_a_root_only_token_file() {
    let deploy = read("deploy/pi/deploy.sh");
    for line in [
        "install -m 0600 -o root -g root \"$staging/tunnel.toml\" /etc/vyajena/tunnel.toml",
        "install -m 0755 -o root -g root \"$staging/rathole\" /usr/local/bin/rathole",
        "\"$RENDER\" pi-tunnel | pi \"umask 077; cat > '$staging/tunnel.toml'\"",
        "systemctl restart vyajena-tunnel.service",
        "mv -f \"$bin\" \"$bin.prev\"",
    ] {
        assert!(deploy.contains(line), "deploy.sh lacks {line}");
    }
}

// spec: pi-deployment/scripted-reset
#[test]
fn a_reset_needs_the_server_id_and_a_backup_before_anything_is_deleted() {
    let sandbox = Sandbox::new().with_secrets();

    // A wrong answer: nothing runs on the Pi, not even the backup.
    let out = sandbox.run("deploy/pi/deploy.sh", &["--no-build", "--reset"], "yes\n");
    assert!(!out.status.success());
    assert!(
        text(&out).contains("not confirmed; nothing changed"),
        "{}",
        text(&out)
    );
    assert!(sandbox.calls().is_empty(), "{}", sandbox.calls());

    // The right answer, but the backup fails (the fake ssh): the deploy stops
    // before opening its own session to the Pi.
    let out = sandbox.run("deploy/pi/deploy.sh", &["--no-build", "--reset"], "pi\n");
    assert!(!out.status.success());
    assert!(
        text(&out).contains("the backup failed; nothing was deleted"),
        "{}",
        text(&out)
    );
    let calls = sandbox.calls();
    assert_eq!(calls.lines().count(), 1, "{calls}");
    assert!(
        calls.contains("sudo bash -s"),
        "only backup.sh's ssh ran: {calls}"
    );

    // The wipe lives only in the deploy script's remote install, behind reset.
    let deploy = read("deploy/pi/deploy.sh");
    assert!(deploy.contains("find /var/lib/vyajena -mindepth 1 -delete"));
    assert!(deploy.contains("if [ \"$reset\" = yes ]; then\n    remote_wipe"));
    assert!(deploy.contains("bootstrap-invite-"));
}

// spec: pi-deployment/hardening-check
#[test]
fn the_pi_check_covers_the_tunnel_and_the_relay_path() {
    let check = read("deploy/pi/check.sh");
    for name in [
        "tunnel-running",
        "tunnel sandbox",
        "tunnel attached",
        "nftables no 8443",
        "lan-closed",
        "relay-pin",
        "listening 127.0.0.1:8443",
        "/etc/vyajena/tunnel.toml:\"600 root\"",
        "kernel-reboot",
    ] {
        assert!(check.contains(name), "pi check.sh has no {name:?} check");
    }
}
