use dembly_runtime::{
    ensure_volume_targets_exist, run_checks, ResolvedRuntimeUser, RuntimeCard, RuntimeCheck,
    RuntimeConfig, RuntimeUserSpec,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn checks_run_in_config_order_from_card_cwd_with_planned_environment() {
    let root = temporary_directory("check-order");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let log = root.join("checks.log");
    let script = root.join("check.sh");
    fs::write(
        &script,
        "#!/bin/sh\nprintf '%s|%s|%s|%s\\n' \"$1\" \"$PWD\" \"$PLANNED\" \"${UNPLANNED-unset}\" >> \"$2\"\n",
    )
    .unwrap();
    make_executable(&script);

    let mut config = config();
    config.cards = vec![card("first", first.clone()), card("second", second.clone())];
    config.environment.insert("PLANNED".into(), "yes".into());
    config.checks = vec![
        check("second", &script, "2", &log),
        check("first", &script, "1", &log),
    ];

    run_checks(&config, &user()).unwrap();

    assert_eq!(
        fs::read_to_string(log).unwrap(),
        format!(
            "1|{}|yes|unset\n2|{}|yes|unset\n",
            first.display(),
            second.display()
        )
    );
}

#[test]
fn check_failure_stops_later_checks_and_names_card() {
    let root = temporary_directory("check-failure");
    let card_root = root.join("card");
    fs::create_dir_all(&card_root).unwrap();
    let marker = root.join("later-ran");
    let mut config = config();
    config.cards = vec![card("clang", card_root)];
    config.checks = vec![
        RuntimeCheck {
            card: "clang".into(),
            exec: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 17".into()],
        },
        RuntimeCheck {
            card: "clang".into(),
            exec: "/bin/sh".into(),
            args: vec!["-c".into(), format!("touch {}", marker.display())],
        },
    ];

    let error = run_checks(&config, &user()).unwrap_err();

    assert!(error.contains("clang"), "{error}");
    assert!(error.contains("17"), "{error}");
    assert!(!marker.exists());
}

#[test]
fn every_compose_volume_target_must_exist() {
    let root = temporary_directory("volume-targets");
    let present = root.join("present");
    let missing = root.join("missing");
    fs::create_dir_all(&present).unwrap();

    let error = ensure_volume_targets_exist(&[present, missing.clone()]).unwrap_err();

    assert!(error.contains("Volume 1"), "{error}");
    assert!(
        error.contains(&missing.to_string_lossy().to_string()),
        "{error}"
    );
}

fn config() -> RuntimeConfig {
    RuntimeConfig {
        schema_version: 1,
        lock_digest: "sha256:lock".into(),
        runtime_user: RuntimeUserSpec {
            spec: "vscode".into(),
        },
        cards: Vec::new(),
        binds: Vec::new(),
        exports: Vec::new(),
        hooks: Vec::new(),
        checks: Vec::new(),
        environment: BTreeMap::new(),
        process_argv: vec!["/bin/sh".into()],
    }
}

fn card(name: &str, mount_target: PathBuf) -> RuntimeCard {
    RuntimeCard {
        name: name.into(),
        image: "/tmp/card.squashfs".into(),
        mount_target,
    }
}

fn check(card: &str, script: &std::path::Path, order: &str, log: &std::path::Path) -> RuntimeCheck {
    RuntimeCheck {
        card: card.into(),
        exec: script.into(),
        args: vec![order.into(), log.to_string_lossy().into_owned()],
    }
}

fn user() -> ResolvedRuntimeUser {
    ResolvedRuntimeUser {
        name: "vscode".into(),
        uid: 1000,
        gid: 1000,
        home: "/home/vscode".into(),
    }
}

fn temporary_directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dembly-runtime-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}
