use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
    fs::write(root.join("deck.toml"), format!("schema_version = 1\nname = \"fixture-{}\"\n[base]\nimage = \"{tag}\"\n[[cards]]\npath = \"cards/hello/card.toml\"\n", std::process::id())).unwrap();

    run(Command::new(&binary).current_dir(&root).arg("validate"));
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "hello"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello-card");
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
