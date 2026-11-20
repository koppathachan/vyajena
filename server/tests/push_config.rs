//! server-runtime: Push configuration. `push.toml` and the credentials folder:
//! owner-only modes, at most one `.p8`, keys that parse; a missing key leaves
//! its platform on the logging provider.

mod push_support;

use push_support::{
    FakeServer, apns_key, credentials_dir, service_account_json, set_mode, write_key,
};
use server::push::{Credentials, Options, Providers, PushConfigError, PushSettings};

const GOOD: &str = r#"
credentials_dir = "creds"
proxy = "http://127.0.0.1:8445"
apns_team_id = "XMVHK3YASX"
apns_topic = "com.purplechaamba.vyajena"
"#;

fn field_of(error: PushConfigError) -> String {
    match error {
        PushConfigError::Field { field, .. } => field,
        other => panic!("not a field error: {other}"),
    }
}

// spec: server-runtime/push-configuration
#[test]
fn push_toml_is_checked_field_by_field() {
    let base = std::path::Path::new("/etc/vyajena");
    let settings = PushSettings::from_toml(GOOD, base).expect("good");
    assert_eq!(settings.credentials_dir, base.join("creds"));
    assert_eq!(settings.proxy.as_deref(), Some("http://127.0.0.1:8445"));

    let bad = [
        (
            GOOD.replace("credentials_dir = \"creds\"\n", ""),
            "credentials_dir",
        ),
        (GOOD.replace("XMVHK3YASX", "short"), "apns_team_id"),
        (
            GOOD.replace("http://127.0.0.1:8445", "https://127.0.0.1:8445"),
            "proxy",
        ),
        (
            GOOD.replace("http://127.0.0.1:8445", "http://127.0.0.1"),
            "proxy",
        ),
        (
            GOOD.replace("com.purplechaamba.vyajena", "com/evil"),
            "apns_topic",
        ),
        (format!("{GOOD}typo = 1\n"), "typo"),
        (GOOD.replace("\"creds\"", "7"), "credentials_dir"),
    ];
    for (text, want) in bad {
        let error = PushSettings::from_toml(&text, base).expect_err(want);
        assert_eq!(field_of(error), want);
    }
}

// spec: server-runtime/push-configuration
#[tokio::test]
async fn key_files_must_be_owner_only_parse_and_be_unique() {
    let oauth = FakeServer::start().await;

    // No folder, or an empty one: no provider, nothing refused.
    let gone = Credentials::load(&credentials_dir().path().join("missing")).expect("missing");
    assert!(gone.fcm.is_none() && gone.apns.is_none());
    let dir = credentials_dir();
    let creds = Credentials::load(dir.path()).expect("empty");
    assert!(creds.fcm.is_none() && creds.apns.is_none());

    // Both keys; the APNs key id comes from the file name.
    write_key(
        dir.path(),
        "fcm-service-account.json",
        &service_account_json(&oauth),
    );
    write_key(dir.path(), "AuthKey_ABC123DEFG.p8", &apns_key().0);
    let creds = Credentials::load(dir.path()).expect("both");
    assert_eq!(
        creds.fcm.as_ref().map(|f| f.project_id.as_str()),
        Some("vyajena-test")
    );
    assert_eq!(
        creds.apns.as_ref().map(|a| a.key_id.as_str()),
        Some("ABC123DEFG")
    );

    // "Key readable by others".
    set_mode(&dir.path().join("fcm-service-account.json"), 0o644);
    let error = Credentials::load(dir.path()).expect_err("0644 key");
    assert!(
        matches!(&error, PushConfigError::Mode { path, mode: 0o644 } if path.ends_with("fcm-service-account.json")),
        "{error}"
    );
    set_mode(&dir.path().join("fcm-service-account.json"), 0o600);
    set_mode(dir.path(), 0o750);
    assert!(matches!(
        Credentials::load(dir.path()),
        Err(PushConfigError::Mode { .. })
    ));
    set_mode(dir.path(), 0o700);

    // Two .p8 files.
    write_key(dir.path(), "AuthKey_ZZZ999YYYX.p8", &apns_key().0);
    assert!(matches!(
        Credentials::load(dir.path()),
        Err(PushConfigError::Key { .. })
    ));
    std::fs::remove_file(dir.path().join("AuthKey_ZZZ999YYYX.p8")).expect("rm");

    // Keys that do not parse; the error never quotes the key.
    let bad = [
        (
            "fcm-service-account.json",
            "{\"type\":\"service_account\"}".to_owned(),
        ),
        (
            "fcm-service-account.json",
            service_account_json(&oauth).replace("service_account", "user"),
        ),
        (
            "AuthKey_ABC123DEFG.p8",
            "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n".to_owned(),
        ),
    ];
    for (name, text) in bad {
        let dir = credentials_dir();
        write_key(dir.path(), name, &text);
        let error = Credentials::load(dir.path()).expect_err(name);
        assert!(
            matches!(error, PushConfigError::Key { .. }),
            "{name}: {error}"
        );
    }
    let dir = credentials_dir();
    write_key(dir.path(), "AuthKey_bad.p8", &apns_key().0);
    assert!(
        matches!(
            Credentials::load(dir.path()),
            Err(PushConfigError::Key { .. })
        ),
        "key id"
    );
}

// spec: server-runtime/push-configuration
#[tokio::test]
async fn providers_load_from_push_toml() {
    let oauth = FakeServer::start().await;
    let root = credentials_dir();
    let creds = root.path().join("creds");
    std::fs::create_dir(&creds).expect("mkdir");
    set_mode(&creds, 0o700);
    write_key(
        &creds,
        "fcm-service-account.json",
        &service_account_json(&oauth),
    );
    let path = root.path().join("push.toml");
    std::fs::write(&path, GOOD).expect("push.toml");
    let push = Providers::load(&path, &Options::default()).expect("load");
    assert!(push.has_fcm());
    assert!(!push.has_apns());

    let missing = Providers::load(&root.path().join("nope.toml"), &Options::default());
    assert!(matches!(missing, Err(PushConfigError::Read { .. })));
}
