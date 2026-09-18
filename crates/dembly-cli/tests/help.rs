#[test]
fn global_version_is_bare_package_version() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("--version")
        .output()
        .expect("dembly should start");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn public_help_lists_host_commands_without_legacy_lifecycle_commands() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("--help")
        .output()
        .expect("dembly should start");

    let stdout = String::from_utf8(result.stdout).expect("help should be UTF-8");
    assert!(result.status.success());
    for command in [
        "init", "validate", "lock", "apply", "unapply", "inspect", "check", "card",
    ] {
        assert!(stdout.contains(command), "missing {command} in {stdout}");
    }
    for command in ["up", "down", "run", "exec", "__runtime"] {
        assert!(
            !stdout.contains(command),
            "unexpected {command} in {stdout}"
        );
    }
}

#[test]
fn config_commands_reject_legacy_and_malformed_arguments() {
    for arguments in [
        vec!["validate", "deck.toml"],
        vec!["validate", "--config", "one.toml", "--config", "two.toml"],
        vec!["lock", "--unknown"],
        vec!["apply", "--config"],
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
            .args(&arguments)
            .output()
            .expect("dembly should start");
        assert_eq!(
            result.status.code(),
            Some(2),
            "arguments={arguments:?}, stderr={}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn internal_runtime_probe_remains_available_but_hidden() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .args(["__runtime", "probe", "root"])
        .output()
        .expect("dembly should start");
    let output = String::from_utf8(result.stdout).expect("probe output should be UTF-8");
    let fields = output.trim().split('\t').collect::<Vec<_>>();
    assert!(result.status.success());
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0], "root");
}

#[test]
fn card_build_non_interactive_rejects_missing_required_options() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .args(["card", "build", "/tool", "/cards", "--non-interactive"])
        .output()
        .expect("dembly should start");

    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8(result.stderr).unwrap().contains("--name"));
}
