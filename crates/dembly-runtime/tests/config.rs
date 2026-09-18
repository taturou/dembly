use dembly_runtime::{load_runtime_config, render_runtime_config};
use std::fs;
use std::path::PathBuf;

const COMPLETE_CONFIG: &str = r#"schema_version = 1
lock_digest = "sha256:lock"

[runtime_user]
spec = "vscode"

[[cards]]
name = "clang"
image = "/run/dembly/cards/clang.squashfs"
mount_target = "/opt/dembly/cards/clang"

[[binds]]
source = "/run/dembly/binds/0"
target = "${HOME}/.config/${USER}"
mode = "ro"

[[exports]]
source = "/opt/dembly/cards/clang/bin/clang"
target = "/usr/local/bin/clang"

[[hooks]]
card = "clang"
exec = "/opt/dembly/cards/clang/setup/post-mount.sh"
args = ["--install"]

[[checks]]
card = "clang"
exec = "/opt/dembly/cards/clang/bin/clang"
args = ["--version"]

[environment]
PATH = "/opt/dembly/cards/clang/bin:/usr/bin"

[process]
argv = ["/usr/local/bin/start", "--watch"]
"#;

#[test]
fn strict_runtime_config_round_trips_all_plan_sections_deterministically() {
    let path = fixture("round-trip", COMPLETE_CONFIG);
    let config = load_runtime_config(&path).unwrap();

    assert_eq!(config.schema_version, 1);
    assert_eq!(config.lock_digest, "sha256:lock");
    assert_eq!(config.runtime_user.spec, "vscode");
    assert_eq!(config.cards[0].name, "clang");
    assert_eq!(config.binds[0].target, "${HOME}/.config/${USER}");
    assert_eq!(config.checks[0].card, "clang");
    assert_eq!(config.process_argv, ["/usr/local/bin/start", "--watch"]);

    let first = render_runtime_config(&config).unwrap();
    let second = render_runtime_config(&config).unwrap();
    assert_eq!(first, second);
    let rendered = fixture("rendered", &first);
    assert_eq!(load_runtime_config(&rendered).unwrap(), config);
}

#[test]
fn runtime_config_rejects_unknown_fields_at_every_level() {
    for (name, invalid) in [
        (
            "root",
            COMPLETE_CONFIG.replacen("lock_digest =", "unknown = true\nlock_digest =", 1),
        ),
        (
            "nested",
            COMPLETE_CONFIG.replacen("spec = \"vscode\"", "spec = \"vscode\"\nuid = 1000", 1),
        ),
        (
            "array",
            COMPLETE_CONFIG.replacen("name = \"clang\"", "name = \"clang\"\nunknown = true", 1),
        ),
    ] {
        let error = load_runtime_config(&fixture(name, &invalid)).unwrap_err();
        assert!(error.contains("unknown field"), "{error}");
    }
}

#[test]
fn runtime_config_requires_supported_schema_and_nonempty_lock_digest() {
    let missing = COMPLETE_CONFIG.replacen("schema_version = 1\n", "", 1);
    let error = load_runtime_config(&fixture("missing-schema", &missing)).unwrap_err();
    assert!(error.contains("schema_version"), "{error}");

    let invalid = COMPLETE_CONFIG.replacen("schema_version = 1", "schema_version = 2", 1);
    let error = load_runtime_config(&fixture("invalid-schema", &invalid)).unwrap_err();
    assert!(error.contains("schema_version must be 1"), "{error}");

    let empty = COMPLETE_CONFIG.replacen("sha256:lock", "", 1);
    let error = load_runtime_config(&fixture("empty-lock", &empty)).unwrap_err();
    assert!(error.contains("lock_digest"), "{error}");
}

fn fixture(name: &str, contents: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "dembly-runtime-config-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("runtime.toml");
    fs::write(&path, contents).unwrap();
    path
}
