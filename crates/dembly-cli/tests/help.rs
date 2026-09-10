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

#[test]
fn card_build_non_interactive_rejects_missing_required_options() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .args(["card", "build", "/tool", "/cards", "--non-interactive"])
        .output()
        .expect("dembly should start");

    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8(result.stderr).unwrap().contains("--name"));
}

#[test]
fn validate_checks_card_manifest_and_filesystem_from_deck_root() {
    let directory =
        std::env::temp_dir().join(format!("dembly-cli-validate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("cards/clang")).unwrap();
    std::fs::write(directory.join("cards/clang/rootfs.squashfs"), b"abc").unwrap();
    std::fs::write(
        directory.join("cards/clang/card.toml"),
        "schema_version = 1\nname = \"clang\"\nversion = \"20\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n",
    ).unwrap();
    std::fs::write(
        directory.join("deck.toml"),
        "schema_version = 1\nname = \"example\"\n[base]\nimage = \"alpine:3.20\"\n[[cards]]\npath = \"cards/clang/card.toml\"\n",
    ).unwrap();

    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("validate")
        .current_dir(&directory)
        .output()
        .expect("dembly should start");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn inspect_displays_deck_name_base_and_card_without_runtime() {
    let directory = std::env::temp_dir().join(format!("dembly-cli-inspect-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("cards/clang")).unwrap();
    std::fs::write(directory.join("cards/clang/rootfs.squashfs"), b"abc").unwrap();
    std::fs::write(directory.join("cards/clang/card.toml"), "schema_version = 1\nname = \"clang\"\nversion = \"20\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n").unwrap();
    std::fs::write(directory.join("deck.toml"), "schema_version = 1\nname = \"example\"\n[base]\nimage = \"alpine:3.20\"\n[[cards]]\npath = \"cards/clang/card.toml\"\n").unwrap();

    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("inspect")
        .current_dir(&directory)
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(stdout.contains("Deck: example"));
    assert!(stdout.contains("Image Base: alpine:3.20"));
    assert!(stdout.contains("Card: clang 20"));
}
