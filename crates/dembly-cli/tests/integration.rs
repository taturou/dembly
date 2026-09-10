use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn image_base_card_runs_without_host_squashfs_mount() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = repository.join("tests/fixtures/hello-card/rootfs");
    let image_dockerfile = repository.join("tests/fixtures/image-base");
    let tag = format!("dembly-fixture-{}", std::process::id());
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    run(Command::new("docker").args(["build", "-t", &tag, image_dockerfile.to_str().unwrap()]));
    run(Command::new(&binary).args([
        "card",
        "build",
        fixture.to_str().unwrap(),
        root.join("cards").to_str().unwrap(),
        "--name",
        "hello",
        "--version",
        "1",
        "--mount-target",
        "/opt/dembly/cards/hello",
        "--path-prepend",
        "bin",
        "--non-interactive",
    ]));
    let manifest_path = root.join("cards/hello/card.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        format!(
            "{manifest}\n[environment]\nHELLO_ENV = \"enabled\"\n\n[[exports]]\nsource = \"bin/hello\"\ntarget = \"/usr/local/bin/hello\"\n\n[[hooks.post_mount]]\nexec = \"setup/post-mount.sh\"\n\n[check]\nexec = \"bin/hello\"\n"
        ),
    )
    .unwrap();
    fs::write(root.join("host-input"), b"host-input\n").unwrap();
    fs::write(root.join("deck.toml"), format!("schema_version = 1\nname = \"fixture-{}\"\n[base]\nimage = \"{tag}\"\n[[cards]]\npath = \"cards/hello/card.toml\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/work/cache\"\n[[binds]]\nsource = \"${{DECK_ROOT}}/host-input\"\ntarget = \"/work/input\"\nmode = \"ro\"\n", std::process::id())).unwrap();

    run(Command::new(&binary).current_dir(&root).arg("validate"));
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/usr/local/bin/hello"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello-card");
    let second_output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/usr/local/bin/hello"])
        .output()
        .unwrap();
    assert!(
        second_output.status.success(),
        "{}",
        String::from_utf8_lossy(&second_output.stderr)
    );
    let check_output = Command::new(&binary)
        .current_dir(&root)
        .arg("check")
        .output()
        .unwrap();
    assert!(
        check_output.status.success(),
        "{}",
        String::from_utf8_lossy(&check_output.stderr)
    );
    assert!(String::from_utf8_lossy(&check_output.stdout).contains("check hello"));
    assert_eq!(
        fs::read_to_string(root.join("volumes/cache/result")).unwrap(),
        "persisted\npersisted\npersisted\n"
    );
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("exec = \"bin/hello\"", "exec = \"bin/missing\""),
    )
    .unwrap();
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let failed_check = Command::new(&binary)
        .current_dir(&root)
        .arg("check")
        .output()
        .unwrap();
    assert!(!failed_check.status.success());
    let filesystem_path = root.join("cards/hello/rootfs.squashfs");
    let mut filesystem = fs::read(&filesystem_path).unwrap();
    filesystem.push(0);
    fs::write(&filesystem_path, filesystem).unwrap();
    let tampered_run = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/true"])
        .output()
        .unwrap();
    assert!(!tampered_run.status.success());
    assert!(String::from_utf8_lossy(&tampered_run.stderr).contains("checksum"));
    let host_mounts = fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(!host_mounts.contains("/opt/dembly/cards/hello"));
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", &tag])
        .status();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_base_runs_a_temporary_command_and_cleans_up() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let deck_name = format!("compose-fixture-{}", std::process::id());

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::write(
        root.join("compose.yaml"),
        "services:\n  dev:\n    image: alpine:3.21\n    command: [\"sleep\", \"infinity\"]\n",
    )
    .unwrap();
    fs::write(
        root.join("deck.toml"),
        format!(
            "schema_version = 1\nname = \"{deck_name}\"\n[base]\ncompose = \"compose.yaml\"\nservice = \"dev\"\n"
        ),
    )
    .unwrap();

    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/echo", "compose-runtime"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("compose-runtime"));
    run(Command::new(&binary).current_dir(&root).arg("up"));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["exec", "--", "/bin/echo", "compose-exec"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "compose-exec"
    );
    run(Command::new(&binary).current_dir(&root).arg("down"));
    fs::write(
        root.join("compose.yaml"),
        "services:\n  dev:\n    image: alpine:3.21\n    command: [\"/bin/true\"]\n",
    )
    .unwrap();
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    run(Command::new(&binary).current_dir(&root).arg("check"));
    let project = format!("dembly-{deck_name}");
    let containers = Command::new("docker")
        .args(["compose", "-p", &project, "-f"])
        .arg(root.join("compose.yaml"))
        .args(["ps", "--all", "-q"])
        .output()
        .unwrap();
    assert!(containers.status.success());
    assert!(containers.stdout.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn card_build_interactive_creates_an_artifact_from_prompted_values() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let tool_root = root.join("interactive-tool");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::create_dir_all(tool_root.join("bin")).unwrap();
    fs::write(tool_root.join("bin/tool"), b"#!/bin/sh\nexit 0\n").unwrap();
    let mut child = Command::new(&binary)
        .args(["card", "build"])
        .arg(&tool_root)
        .arg(root.join("cards"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\n1.0.0\n\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(root.join("cards/interactive-tool/card.toml")).unwrap();
    assert!(manifest.contains("name = \"interactive-tool\""));
    assert!(manifest.contains("version = \"1.0.0\""));
    assert!(root
        .join("cards/interactive-tool/rootfs.squashfs")
        .is_file());
    let _ = fs::remove_dir_all(root);
}

fn temporary_directory() -> PathBuf {
    let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "dembly-integration-{}-{sequence}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
