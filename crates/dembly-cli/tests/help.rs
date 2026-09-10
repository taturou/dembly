#[test]
fn public_help_lists_validate_without_internal_runtime_namespace() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("--help")
        .output()
        .expect("dembly should start");

    let stdout = String::from_utf8(result.stdout).expect("help should be UTF-8");
    assert!(result.status.success());
    assert!(stdout.contains("validate"));
    assert!(!stdout.contains("__runtime"));
}
