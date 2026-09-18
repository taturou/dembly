use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn interactive_card_build_reads_prompts_and_reports_artifact_checksum() {
    let project = TemporaryProject::new("interactive-card");
    let tool_root = project.root.join("interactive-tool");
    let cards_root = project.root.join("cards");
    fs::create_dir_all(tool_root.join("bin")).unwrap();
    let tool = tool_root.join("bin/tool");
    fs::write(&tool, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&tool).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&tool, permissions).unwrap();
    let original_tool = fs::read(&tool).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_dembly"))
        .args(["card", "build"])
        .arg(&tool_root)
        .arg(&cards_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\n1.2.3\n\n")
        .unwrap();

    let output = wait_with_timeout(child, Duration::from_secs(10));

    assert_success(&output);
    let card_root = cards_root.join("interactive-tool");
    let manifest = fs::read_to_string(card_root.join("card.toml")).unwrap();
    assert!(manifest.contains("name = \"interactive-tool\""));
    assert!(manifest.contains("version = \"1.2.3\""));
    assert!(manifest.contains("target = \"/opt/dembly/cards/interactive-tool\""));
    assert!(card_root.join("rootfs.squashfs").is_file());
    assert_eq!(fs::read(&tool).unwrap(), original_tool);
    let checksum = manifest
        .lines()
        .find_map(|line| line.strip_prefix("sha256 = \"")?.strip_suffix('"'))
        .unwrap();
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains(&card_root.display().to_string()),
        "{stdout}"
    );
    assert!(stdout.contains(checksum), "{stdout}");
}

#[test]
fn validate_rejects_a_missing_required_host_bind() {
    let project = TemporaryProject::new("missing-bind");
    write_compose(&project.root);
    write_config(
        &project.root,
        "[[binds]]\nsource = \"missing\"\ntarget = \"/work/input\"\nmode = \"ro\"\n",
    );

    let output = validate(&project.root);

    assert_failure(&output, "missing required Host Bind source");
    assert!(text(&output.stderr).contains(".dembly/missing"));
}

#[test]
fn validate_rejects_duplicate_export_targets_across_cards() {
    let project = TemporaryProject::new("duplicate-export");
    write_compose(&project.root);
    for name in ["one", "two"] {
        write_card(&project.root, name, "/usr/local/bin/shared-tool");
    }
    write_config(
        &project.root,
        "[[cards]]\npath = \"cards/one/card.toml\"\n[[cards]]\npath = \"cards/two/card.toml\"\n",
    );

    let output = validate(&project.root);

    assert_failure(
        &output,
        "duplicate export target: /usr/local/bin/shared-tool",
    );
}

struct TemporaryProject {
    root: PathBuf,
}

impl TemporaryProject {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for TemporaryProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_compose(root: &Path) {
    fs::write(
        root.join("compose.yaml"),
        "name: focused\nservices:\n  dev:\n    image: alpine:3.21\n",
    )
    .unwrap();
}

fn write_config(root: &Path, declarations: &str) {
    let deck_root = root.join(".dembly");
    fs::create_dir_all(&deck_root).unwrap();
    fs::write(
        deck_root.join("config.toml"),
        format!(
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n{declarations}"
        ),
    )
    .unwrap();
}

fn write_card(root: &Path, name: &str, export_target: &str) {
    let card_root = root.join(".dembly/cards").join(name);
    fs::create_dir_all(&card_root).unwrap();
    fs::write(card_root.join("rootfs.squashfs"), b"abc").unwrap();
    fs::write(
        card_root.join("card.toml"),
        format!(
            "schema_version = 1\nname = \"{name}\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/{name}\"\n[[exports]]\nsource = \"bin/tool\"\ntarget = \"{export_target}\"\n"
        ),
    )
    .unwrap();
}

fn validate(root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dembly"))
        .current_dir(root)
        .arg("validate")
        .output()
        .unwrap()
}

fn wait_with_timeout(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "card build did not finish: stdout={} stderr={}",
                text(&output.stdout),
                text(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn assert_failure(output: &Output, expected: &str) {
    assert!(!output.status.success(), "stdout={}", text(&output.stdout));
    assert!(
        text(&output.stderr).contains(expected),
        "stderr={}",
        text(&output.stderr)
    );
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
