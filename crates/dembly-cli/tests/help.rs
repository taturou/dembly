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
    assert!(
        stdout.contains("check     Verify static Host integrity"),
        "check must not promise to execute Runtime Card checks: {stdout}"
    );
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
        vec!["validate", "unexpected.toml"],
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
fn malformed_internal_runtime_commands_exit_two_and_probe_is_removed() {
    for arguments in [
        vec!["__runtime"],
        vec!["__runtime", "probe", "root"],
        vec!["__runtime", "init"],
        vec!["__runtime", "check"],
        vec!["__runtime", "check", "/plan.toml", "extra"],
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
            .args(&arguments)
            .output()
            .expect("dembly should start");
        assert_eq!(
            result.status.code(),
            Some(2),
            "arguments={arguments:?}, stdout={}, stderr={}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("invalid internal runtime command")
        );
    }
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
