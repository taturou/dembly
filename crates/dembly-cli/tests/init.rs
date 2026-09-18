use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn one_card_candidate_requires_an_explicit_adoption_confirmation() {
    let root = temporary_project("confirmation");
    write_card(&root, "cards/clang", "clang");
    write_compose(&root, "compose.yaml", "dev");

    let rejected = run_init(&root, "1\nn\ncompose.yaml\ndev\n");
    assert!(rejected.status.success());
    let rejected_config = fs::read_to_string(root.join(".dembly/config.toml")).unwrap();
    assert!(!rejected_config.contains("[[cards]]"));

    fs::remove_file(root.join(".dembly/config.toml")).unwrap();
    let accepted = run_init(&root, "1\ny\ncompose.yaml\ndev\n");
    assert!(accepted.status.success());
    let accepted_config = fs::read_to_string(root.join(".dembly/config.toml")).unwrap();
    assert!(accepted_config.contains("path = \"../cards/clang/card.toml\""));
}

#[test]
fn selected_cards_are_rendered_in_selected_order() {
    let root = temporary_project("card-order");
    write_card(&root, "cards/alpha", "alpha");
    write_card(&root, "cards/beta", "beta");
    write_compose(&root, "compose.yaml", "dev");

    let result = run_init(&root, "2,1\ny\ny\ncompose.yaml\ndev\n");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let config = fs::read_to_string(root.join(".dembly/config.toml")).unwrap();
    assert!(
        config.find("../cards/beta/card.toml").unwrap()
            < config.find("../cards/alpha/card.toml").unwrap()
    );
}

#[test]
fn duplicate_selected_card_names_fail_without_writing_config() {
    let root = temporary_project("duplicate-cards");
    write_card(&root, "cards/one", "same");
    write_card(&root, "cards/two", "same");

    let result = run_init(&root, "1,2\n");
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("duplicate Card name"));
    assert!(!root.join(".dembly/config.toml").exists());
}

#[test]
fn selected_devcontainer_shows_service_and_compose_sequence_before_confirmation() {
    let root = temporary_project("devcontainer");
    fs::create_dir_all(root.join(".devcontainer")).unwrap();
    write_compose(&root, ".devcontainer/compose.base.yaml", "dev");
    write_compose(&root, ".devcontainer/compose.yaml", "dev");
    fs::write(
        root.join(".devcontainer/devcontainer.json"),
        r#"{"dockerComposeFile":["compose.base.yaml","compose.yaml"],"service":"dev","remoteUser":"vscode"}"#,
    )
    .unwrap();

    let result = run_init(&root, "1\ny\n");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("Service: dev"));
    assert!(stdout.contains("compose.base.yaml"));
    assert!(stdout.contains("compose.yaml"));
    let config = fs::read_to_string(root.join(".dembly/config.toml")).unwrap();
    assert!(config.contains("path = \"../.devcontainer/devcontainer.json\""));
    assert!(config.contains("path = \"../.devcontainer/compose.yaml\""));
}

#[test]
fn no_devcontainer_prompts_for_compose_path_and_service() {
    let root = temporary_project("manual-compose");
    write_compose(&root, "compose.yaml", "workspace");

    let result = run_init(&root, "compose.yaml\nworkspace\n");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("Compose path"));
    assert!(stdout.contains("Compose service"));
    let config = fs::read_to_string(root.join(".dembly/config.toml")).unwrap();
    assert!(config.contains("path = \"../compose.yaml\""));
    assert!(config.contains("service = \"workspace\""));
    assert_config_resolves_to(
        &root.join(".dembly/config.toml"),
        &root.join("compose.yaml"),
    );
}

#[test]
fn existing_config_is_unchanged() {
    let root = temporary_project("existing");
    fs::create_dir_all(root.join(".dembly")).unwrap();
    fs::write(root.join(".dembly/config.toml"), "preserve this content\n").unwrap();

    let result = run_init(&root, "");
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(root.join(".dembly/config.toml")).unwrap(),
        "preserve this content\n"
    );
}

#[test]
fn custom_config_parent_is_created_and_cards_are_relative_to_it() {
    let root = temporary_project("custom-config");
    write_card(&root, "cards/clang", "clang");
    write_compose(&root, "compose.yaml", "dev");

    let child = Command::new(env!("CARGO_BIN_EXE_dembly"))
        .args(["init", "--config", "custom/config.toml"])
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let output = with_input(child, "1\ny\ncompose.yaml\ndev\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config = fs::read_to_string(root.join("custom/config.toml")).unwrap();
    assert!(config.contains("path = \"../cards/clang/card.toml\""));
    assert!(config.contains("path = \"../compose.yaml\""));
}

#[test]
fn custom_config_compose_path_is_relative_to_its_parent_and_resolves_to_cwd_compose() {
    let root = temporary_project("custom-compose");
    write_compose(&root, "compose.yaml", "dev");

    let child = Command::new(env!("CARGO_BIN_EXE_dembly"))
        .args(["init", "--config", "custom/config.toml"])
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let output = with_input(child, "compose.yaml\ndev\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config_path = root.join("custom/config.toml");
    assert!(fs::read_to_string(&config_path)
        .unwrap()
        .contains("path = \"../compose.yaml\""));
    assert_config_resolves_to(&config_path, &root.join("compose.yaml"));
}

#[test]
fn manual_compose_validation_rejects_missing_file_and_service_without_writing_config() {
    let missing_file_root = temporary_project("manual-missing-compose");
    let missing_file = run_init(&missing_file_root, "missing.yaml\ndev\n");
    assert_eq!(missing_file.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_file.stderr).contains("cannot read Compose file"));
    assert!(!missing_file_root.join(".dembly/config.toml").exists());

    let missing_service_root = temporary_project("manual-missing-service");
    write_compose(&missing_service_root, "compose.yaml", "other");
    let missing_service = run_init(&missing_service_root, "compose.yaml\ndev\n");
    assert_eq!(missing_service.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_service.stderr).contains("no service dev"));
    assert!(!missing_service_root.join(".dembly/config.toml").exists());
}

#[test]
fn selected_devcontainer_rejects_missing_compose_file_or_service_without_writing_config() {
    for (name, compose_source, expected_error) in [
        (
            "devcontainer-missing-compose",
            None,
            "cannot read Compose file",
        ),
        (
            "devcontainer-missing-service",
            Some("services:\n  other: {}\n"),
            "no service dev",
        ),
    ] {
        let root = temporary_project(name);
        fs::create_dir_all(root.join(".devcontainer")).unwrap();
        fs::write(
            root.join(".devcontainer/devcontainer.json"),
            r#"{"dockerComposeFile":"compose.yaml","service":"dev","remoteUser":"vscode"}"#,
        )
        .unwrap();
        if let Some(compose_source) = compose_source {
            fs::write(root.join(".devcontainer/compose.yaml"), compose_source).unwrap();
        }

        let result = run_init(&root, "1\ny\n");
        assert_eq!(result.status.code(), Some(2), "project={name}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected_error),
            "project={name}, stderr={}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!root.join(".dembly/config.toml").exists(), "project={name}");
    }
}

fn run_init(root: &Path, input: &str) -> std::process::Output {
    let child = Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("init")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    with_input(child, input)
}

fn with_input(mut child: std::process::Child, input: &str) -> std::process::Output {
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn temporary_project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "dembly-cli-init-{name}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn write_card(root: &Path, relative: &str, name: &str) {
    let card_root = root.join(relative);
    fs::create_dir_all(&card_root).unwrap();
    fs::write(
        card_root.join("card.toml"),
        format!(
            "schema_version = 1\nname = \"{name}\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n[mount]\ntarget = \"/opt/dembly/cards/{name}\"\n"
        ),
    )
    .unwrap();
}

fn write_compose(root: &Path, relative: &str, service: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("services:\n  {service}: {{}}\n")).unwrap();
}

fn assert_config_resolves_to(config_path: &Path, compose_path: &Path) {
    let resolved = dembly_core::resolve_deck(
        config_path,
        &dembly_core::HostVariables {
            host_home: PathBuf::from("/tmp/dembly-test-home"),
            deck_root: config_path.parent().unwrap().to_path_buf(),
        },
    )
    .unwrap();
    assert_eq!(resolved.compose_path, compose_path);
}
