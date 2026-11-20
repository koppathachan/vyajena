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
fn the_relay_is_an_e2_micro_in_mumbai_on_its_own_network() {
    // relay-mumbai: next to the Pi and its users, not a US region.
    let region = conf("REGION");
    assert_eq!(region, "asia-south1");
    assert!(conf("ZONE").starts_with(&format!("{region}-")));
    // The VPC is shared with the old us-east1 subnet during the move, so the
    // new subnet must not reuse its range.
    assert_ne!(conf("SUBNET_RANGE"), "10.42.0.0/28");
    // The relay check enforces the same region.
    assert!(read("deploy/relay/check.sh").contains("RELAY_REGION=\"asia-south1\""));
    assert_eq!(conf("MACHINE_TYPE"), "e2-micro");
    assert_eq!(conf("DISK_TYPE"), "pd-standard");
    let disk: u32 = conf("DISK_GB").parse().check("DISK_GB");
    assert!(disk <= 30, "{disk} GB is over the 30 GB limit");
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
        "--zone=asia-south1-c",
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
    // The address and subnet are made in Mumbai.
    for create in [
        "compute addresses create",
        "compute networks subnets create",
    ] {
        let line = commands
            .iter()
            .find(|c| c.contains(create))
            .check("regional create");
        assert!(line.contains("--region=asia-south1"), "{line}");
    }
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
    assert_eq!(rules.len(), 4, "{rules:#?}");
    let tunnel = conf("TUNNEL_PORT");
    let review = conf("REVIEW_PORT");
    let expect = [
        ("tcp:443", "0.0.0.0/0"),
        (&*format!("tcp:{review}"), "0.0.0.0/0"),
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
        "review-token",
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
    // APIs, network, subnet, 4 rules, address, VM, host keys.
    assert_eq!(gcloud.len(), 10, "{gcloud:#?}");
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
    let server_id = Config::from_toml(&sandbox.render("pi-config"), Path::new("/"))
        .check("rendered deploy/pi/config.toml")
        .server_id;
    let out = sandbox.run(
        "deploy/pi/deploy.sh",
        &["--no-build", "--reset"],
        &format!("{server_id}\n"),
    );
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
    assert!(deploy.contains("kind=bootstrap-invite"));
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

// --- review-server -----------------------------------------------------------

// spec: review-server/separate-review-server
#[test]
fn the_review_server_has_its_own_ids_port_user_and_data() {
    let sandbox = Sandbox::new().with_secrets();
    let review = Config::from_toml(&sandbox.render("pi-review-config"), Path::new("/"))
        .check("rendered deploy/pi/review.toml");
    let real = Config::from_toml(&sandbox.render("pi-config"), Path::new("/"))
        .check("rendered deploy/pi/config.toml");
    assert_eq!(review.admin_user_id(), "review@review.purplechaamba.com");
    assert_eq!(review.listen_address.to_string(), "127.0.0.1:8444");
    assert_ne!(review.server_id, real.server_id);
    assert_ne!(review.listen_address, real.listen_address);
    assert_ne!(review.database_path, real.database_path);
    assert_ne!(review.key_dir, real.key_dir);
    assert!(review.key_dir.starts_with("/var/lib/vyajena-review"));

    // The unit differs from the real one only in user, config and state dir.
    let real_unit = read("deploy/pi/vyajena.service");
    let review_unit = read("deploy/pi/vyajena-review.service");
    for line in [
        "User=vyajena-review",
        "Group=vyajena-review",
        "StateDirectory=vyajena-review",
        "ExecStart=/usr/local/bin/vyajena-server --config /etc/vyajena/review.toml",
    ] {
        assert!(
            review_unit.lines().any(|l| l == line),
            "review unit lacks {line}"
        );
    }
    let settings = |unit: &str| -> Vec<String> {
        let own = [
            "User=",
            "Group=",
            "StateDirectory=",
            "ExecStart=",
            "Description=",
        ];
        unit.lines()
            .filter(|l| !l.starts_with('#') && l.contains('='))
            .filter(|l| !own.iter().any(|k| l.starts_with(k)))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(
        settings(&review_unit),
        settings(&real_unit),
        "same sandbox as the real server"
    );
    assert!(read("deploy/pi/harden.sh").contains("--shell /usr/sbin/nologin vyajena-review"));
}

// spec: review-server/review-route-through-the-relay
#[test]
fn the_review_route_has_its_own_token_and_ports() {
    let sandbox = Sandbox::new().with_secrets();
    let server = toml(&sandbox.render("relay-server"));
    let client = toml(&sandbox.render("pi-tunnel"));
    let review_token = sandbox.secret("review-token");
    assert_ne!(review_token, sandbox.secret("token"));
    assert_eq!(
        string(&server, "server.services.review.token"),
        review_token
    );
    assert_eq!(
        string(&client, "client.services.review.token"),
        review_token
    );
    assert_eq!(
        string(&server, "server.services.review.bind_addr"),
        "0.0.0.0:8443"
    );
    assert_eq!(
        string(&client, "client.services.review.local_addr"),
        "127.0.0.1:8444"
    );
    // Review invites name the relay's DNS name and the review port.
    let review = Config::from_toml(&sandbox.render("pi-review-config"), Path::new("/"))
        .check("review config");
    assert_eq!(review.public_address, "vyajarelay.purplechaamba.com:8443");
    assert_eq!(conf("REVIEW_PORT"), "8443");
}

// spec: review-server/review-reset
#[test]
fn a_review_reset_needs_the_review_id_and_never_touches_the_real_server() {
    let sandbox = Sandbox::new().with_secrets();
    let out = sandbox.run(
        "deploy/pi/deploy.sh",
        &["--no-build", "--reset-review"],
        "pi\n",
    );
    assert!(!out.status.success());
    assert!(
        text(&out).contains("not confirmed; nothing changed"),
        "{}",
        text(&out)
    );
    assert!(sandbox.calls().is_empty(), "{}", sandbox.calls());

    // The right id: no backup is taken; the deploy goes straight to the Pi
    // (the fake ssh refuses, so nothing more happens).
    let out = sandbox.run(
        "deploy/pi/deploy.sh",
        &["--no-build", "--reset-review"],
        "review.purplechaamba.com\n",
    );
    assert!(!out.status.success());
    assert!(text(&out).contains("REVIEW RESET"), "{}", text(&out));
    assert!(!text(&out).contains("backing up"), "{}", text(&out));
    assert!(
        !sandbox.calls().contains("sudo bash -s"),
        "no backup: {}",
        sandbox.calls()
    );

    let deploy = read("deploy/pi/deploy.sh");
    let wipe = deploy
        .split("remote_wipe_review() {")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .check("remote_wipe_review");
    assert!(wipe.contains("find /var/lib/vyajena-review -mindepth 1 -delete"));
    assert!(
        !wipe.contains("/var/lib/vyajena "),
        "the review wipe names the real dir"
    );
    assert!(
        !wipe.contains("vyajena.service"),
        "the review wipe stops the real server"
    );
    assert!(deploy.contains("kind=review-bootstrap-invite"));
}

// spec: review-server/review-server-check
#[test]
fn the_pi_check_covers_the_review_server_and_separate_pins() {
    let check = read("deploy/pi/check.sh");
    for name in [
        "server_checks vyajena-review.service vyajena-review /var/lib/vyajena-review",
        "review listening 127.0.0.1:8444",
        "review-pin",
        "pins differ",
        "lan-closed",
        "/etc/vyajena/review.toml:\"644 root\"",
    ] {
        assert!(check.contains(name), "pi check.sh has no {name:?}");
    }
    let relay_check = read("deploy/relay/check.sh");
    assert!(relay_check.contains("$NAME-review INGRESS false false tcp:$REVIEW_PORT"));
}

// --- media-blobs: the blob store on the Pi ---------------------------------------

/// A `pi.conf` that names `blob_dir` (empty for none).
fn pi_conf_with(sandbox: &Sandbox, blob_dir: &str) -> PathBuf {
    let path = sandbox.dir.path().join("pi.conf");
    // Quoted, so an odd value reaches the script's own checks as text.
    std::fs::write(&path, format!("BLOB_DIR='{blob_dir}'\n")).check("pi.conf");
    path
}

/// `render.sh pi-config` with `blob_dir` in the Pi's settings.
fn render_pi_config(sandbox: &Sandbox, blob_dir: &str) -> Output {
    let conf = pi_conf_with(sandbox, blob_dir);
    let mut command = Command::new("bash");
    command
        .arg(repo().join("deploy/relay/render.sh"))
        .arg("pi-config")
        .env("RELAY_SECRETS", sandbox.secrets())
        .env("PI_CONF", conf)
        .env("HOME", sandbox.dir.path());
    command.output().check("render.sh")
}

/// The one function `name` out of deploy.sh, run with `args`, with fake tools
/// first on PATH that log their calls and print what `fakes` say. Returns
/// the output and the tools' calls, one per line.
fn run_deploy_function(
    name: &str,
    args: &[&str],
    fakes: &[(&str, &str)],
    env: &[(&str, &Path)],
) -> (Output, Vec<String>) {
    let dir = tempfile::tempdir().check("temp dir");
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).check("bin");
    let log = dir.path().join("calls.log");
    for tool in ["systemctl", "findmnt", "chown", "install", "rmdir"] {
        let body = fakes
            .iter()
            .find(|(t, _)| *t == tool)
            .map_or(String::new(), |(_, body)| (*body).to_owned());
        let path = bin.join(tool);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho \"{tool} $*\" >> '{}'\n{body}\n",
                log.display()
            ),
        )
        .check("fake tool");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).check("chmod");
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut command = Command::new("bash");
    command
        .arg("-c")
        .arg(format!(
            "eval \"$(sed -n '/^{name}() {{/,/^}}/p' \"$0\")\"; {name} \"$@\""
        ))
        .arg(repo().join("deploy/pi/deploy.sh"))
        .args(args)
        .env("PATH", path);
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().check(name);
    let calls = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    (out, calls)
}

// spec: pi-deployment/blob-store-deployment
#[test]
fn images_are_off_until_the_owner_names_the_drives_path() {
    // As committed: the owner's drive (mounted at /mnt/vyajena through fstab),
    // so the server's config names that folder.
    let conf = read("deploy/pi/pi.conf");
    assert!(
        conf.lines().any(|l| l == "BLOB_DIR=/mnt/vyajena/blobs"),
        "the committed pi.conf names the owner's drive"
    );
    let sandbox = Sandbox::new().with_secrets();
    let config = Config::from_toml(&sandbox.render("pi-config"), Path::new("/")).check("config");
    assert_eq!(
        config.blob_store,
        Some(server::runtime::config::BlobStoreConfig::Filesystem {
            path: "/mnt/vyajena/blobs".into()
        })
    );

    // With no path: no [blob_store], and the Pi is as before (images off).
    let out = render_pi_config(&sandbox, "");
    assert!(out.status.success(), "{}", text(&out));
    let rendered = String::from_utf8(out.stdout).check("utf-8");
    let config = Config::from_toml(&rendered, Path::new("/")).check("config");
    assert_eq!(config.blob_store, None);

    // With a path: the server's config names exactly it.
    let out = render_pi_config(&sandbox, "/mnt/vyajena-blobs");
    assert!(out.status.success(), "{}", text(&out));
    let rendered = String::from_utf8(out.stdout).check("utf-8");
    let config = Config::from_toml(&rendered, Path::new("/")).check("config");
    assert_eq!(
        config.blob_store,
        Some(server::runtime::config::BlobStoreConfig::Filesystem {
            path: "/mnt/vyajena-blobs".into()
        })
    );
    // The rest of the config is the same as without.
    assert_eq!(config.server_id, "purplechaamba.com");
}

// spec: pi-deployment/blob-store-deployment
#[test]
fn a_blob_path_that_could_put_blobs_on_the_wrong_storage_is_refused() {
    let sandbox = Sandbox::new().with_secrets();
    for bad in [
        "blobs",                  // relative
        "/",                      // the root
        "/var/lib/vyajena/blobs", // what the backup copies and a reset wipes
        "/var/lib/vyajena",
        "/var/lib/vyajena-review/b",
        "/home/vyajan/blobs", // the service cannot see /home
        "/root/blobs",
        "/mnt/a b",
        "/mnt/../etc",
        "/mnt/blobs/", // a trailing slash is not a plain path
        "/mnt/$(id)",
    ] {
        let out = render_pi_config(&sandbox, bad);
        assert!(!out.status.success(), "{bad:?} was accepted");
        assert!(text(&out).contains("BLOB_DIR"), "{bad:?}: {}", text(&out));
    }
    for good in ["/mnt/vyajena-blobs", "/media/drive_1/vyajena", "/srv/blobs"] {
        let out = render_pi_config(&sandbox, good);
        assert!(out.status.success(), "{good:?}: {}", text(&out));
    }
}

// spec: pi-deployment/blob-store-deployment
#[test]
fn deploy_never_makes_the_folder_and_only_hands_over_one_on_its_own_mount() {
    let work = tempfile::tempdir().check("temp dir");
    let dropins = work.path().join("dropins");
    let blobs = work.path().join("drive");
    let env = [("VYAJENA_DROPIN_DIR", dropins.as_path())];
    let dropin = dropins.join("blob-store.conf");

    // The drive is not mounted: its path is not there. The drop-in is written
    // (the service may write the path once it is), a warning says so, and
    // nothing is created or given away.
    let (out, calls) = run_deploy_function(
        "remote_blob_store",
        &[blobs.to_str().check("path")],
        &[("install", "for last; do :; done; mkdir -p \"$last\"")],
        &env,
    );
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("WARN blob store"), "{}", text(&out));
    assert!(!blobs.exists(), "deploy made the folder");
    assert!(!calls.iter().any(|c| c.starts_with("chown")), "{calls:?}");
    let written = std::fs::read_to_string(&dropin).check("drop-in");
    assert!(
        written
            .lines()
            .any(|l| l == format!("ReadWritePaths=-{}", blobs.display())),
        "{written}"
    );
    assert!(written.starts_with("[Service]"));

    // The path exists but is on the main storage (`findmnt` says "/"): the
    // service is not given it.
    std::fs::create_dir(&blobs).check("drive folder");
    let (out, calls) = run_deploy_function(
        "remote_blob_store",
        &[blobs.to_str().check("path")],
        &[("findmnt", "echo /")],
        &env,
    );
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("main storage"), "{}", text(&out));
    assert!(!calls.iter().any(|c| c.starts_with("chown")), "{calls:?}");

    // On a mount of its own: it is the service's, owner-only.
    let (out, calls) = run_deploy_function(
        "remote_blob_store",
        &[blobs.to_str().check("path")],
        &[("findmnt", "echo /mnt/thumb")],
        &env,
    );
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        calls.contains(&format!("chown vyajena:vyajena {}", blobs.display())),
        "{calls:?}"
    );
    assert_eq!(mode(&blobs), 0o700);

    // No path: images are off and the drop-in is gone.
    let (out, _) = run_deploy_function("remote_blob_store", &[""], &[], &env);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!dropin.exists());
}

// spec: pi-deployment/blob-store-deployment
// spec: pi-deployment/sandboxed-service
#[test]
fn the_unit_stays_closed_and_only_the_drop_in_opens_the_blob_path() {
    // The committed unit writes nothing but its state directory...
    let unit = read("deploy/pi/vyajena.service");
    assert!(unit.lines().any(|l| l == "ProtectSystem=strict"));
    assert!(unit.lines().any(|l| l == "StateDirectory=vyajena"));
    assert!(!unit.contains("ReadWritePaths"));
    assert!(
        !unit.contains("RequiresMountsFor"),
        "a missing drive must not stop the Pi"
    );
    // ...and deploy installs the drop-in only when a path is named, and runs
    // the function that does it, on the Pi, for every install.
    let deploy = read("deploy/pi/deploy.sh");
    assert!(deploy.contains("remote_blob_store \"$blob_dir\""));
    assert!(deploy.contains("\"${BLOB_DIR:-}\" | divert_invite"));
    assert!(deploy.contains("remote_push_keys remote_blob_store"));
    // The path is never created by deploy: the function has no mkdir.
    let function: String = deploy
        .lines()
        .skip_while(|l| !l.starts_with("remote_blob_store() {"))
        .take_while(|l| *l != "}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!function.contains("mkdir"), "{function}");
    assert!(!function.contains("install -d -m 0700"), "{function}");
}

// spec: pi-deployment/scripted-reset
#[test]
fn a_reset_deletes_the_groups_blobs_and_only_files_named_like_blobs() {
    let work = tempfile::tempdir().check("temp dir");
    let drive = work.path().join("drive");
    std::fs::create_dir(&drive).check("drive");
    let blob = "0123456789abcdef0123456789abcdef";
    for name in [
        blob,
        ".tmp-0123-ab",
        ".probe-12",
        "notes.txt",
        "0123456789abcdef",
        "0123456789abcdef0123456789abcdeX",
    ] {
        std::fs::write(drive.join(name), b"x").check(name);
    }
    std::fs::create_dir(drive.join("ab".repeat(16))).check("a folder is not a blob");

    let (out, _) = run_deploy_function("remote_wipe", &[drive.to_str().check("path")], &[], &[]);
    assert!(out.status.success(), "{}", text(&out));
    let mut left: Vec<String> = std::fs::read_dir(&drive)
        .check("read_dir")
        .map(|e| e.check("entry").file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "0123456789abcdef",
            "0123456789abcdef0123456789abcdeX",
            "abababababababababababababababab",
            "notes.txt"
        ],
        "only blobs and leftovers went"
    );

    // With no path (images off) or a drive that is gone, nothing is touched.
    let (out, _) = run_deploy_function("remote_wipe", &[""], &[], &[]);
    assert!(out.status.success(), "{}", text(&out));
    let (out, _) = run_deploy_function(
        "remote_wipe",
        &[work.path().join("not-mounted").to_str().check("path")],
        &[],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out));
}

// spec: pi-deployment/backup-to-the-mac
#[test]
fn the_backup_copies_the_database_and_keys_and_never_the_blobs() {
    let backup = read("deploy/pi/backup.sh");
    let code: Vec<&str> = backup
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let code = code.join("\n");
    // The one folder it copies from is /var/lib/vyajena, and deploy refuses a
    // blob path inside it.
    assert!(code.contains("tar -C /var/lib/vyajena -cf - \"backup/$name\" keys"));
    assert!(
        !code.contains("BLOB_DIR") && !code.contains("blob"),
        "{code}"
    );
    assert!(backup.contains("Image blobs are not backed up"));
}

// spec: pi-deployment/hardening-check
#[test]
fn the_pi_check_covers_the_blob_store_and_only_warns_about_an_absent_drive() {
    let check = read("deploy/pi/check.sh");
    for name in [
        "blob-store off (no [blob_store] in the config)",
        "blob-store mount",
        "blob-store sandbox ReadWritePaths",
        "blob-store service can write",
        "blob-store service writes",
    ] {
        assert!(check.contains(name), "pi check.sh has no {name:?} check");
    }
    // An absent drive is a WARN, which never fails the check.
    assert!(check.contains("warn blob-store \"$blob_dir is not there"));
    assert!(!check.contains("fail blob-store \"$blob_dir is not there"));
    // On the main storage is a failure.
    assert!(check.contains("fail \"blob-store mount\""));
}
