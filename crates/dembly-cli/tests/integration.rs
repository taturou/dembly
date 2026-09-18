use serde_yaml::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

static RELEASE_BINARY: OnceLock<PathBuf> = OnceLock::new();

#[test]
fn native_compose_owns_applied_runtime_lifecycle() {
    let binary = release_binary();
    let fixture = Fixture::new();
    fixture.build_image();
    fixture.build_card(&binary);
    fixture.write_config();

    let original_sidecar = fixture.service("sidecar");
    assert_success("dembly lock", &fixture.dembly(&binary, &["lock"]));
    assert_success("dembly apply", &fixture.dembly(&binary, &["apply"]));
    fixture.assert_user_fields_retained(&original_sidecar);
    fixture.assert_devcontainer_configuration();

    assert_success("compose up", &fixture.compose(&["up", "-d"]));
    let first_container = fixture.selected_container();
    let ps = fixture.compose(&["ps", "-a"]);
    assert_success("compose ps", &ps);
    let ps_text = text(&ps.stdout);
    assert!(ps_text.contains(&fixture.project), "ps={ps_text}");
    assert!(ps_text.contains("dev"), "ps={ps_text}");

    let image_id = fixture.image_id();
    fixture.replace_card("version = \"1\"", "version = \"2\"");
    assert_success("updated dembly lock", &fixture.dembly(&binary, &["lock"]));
    assert_success("updated dembly apply", &fixture.dembly(&binary, &["apply"]));
    assert_eq!(
        fixture.image_id(),
        image_id,
        "Card update rebuilt the image"
    );
    fixture.assert_user_fields_retained(&original_sidecar);
    assert_success("updated compose up", &fixture.compose(&["up", "-d"]));
    let second_container = fixture.selected_container();
    assert_ne!(
        second_container, first_container,
        "Card update did not recreate dev"
    );

    let logs = fixture.compose(&["logs", "dev"]);
    assert_success("compose logs", &logs);
    assert!(text(&logs.stdout).contains("original-uid=10001"));

    let exec = fixture.compose(&[
        "exec",
        "-T",
        "--user",
        "dembly",
        "dev",
        "/bin/sh",
        "-c",
        "test \"$(id -u)\" = 10001 && test \"$(cat /tmp/dembly-original-uid)\" = 10001 && test \"$(cat /tmp/dembly-hook-uid)\" = 0 && /usr/local/bin/hello",
    ]);
    assert_success("compose exec", &exec);
    assert!(text(&exec.stdout).contains("hello-card"));

    let run = fixture.compose(&[
        "run",
        "--rm",
        "dev",
        "/bin/sh",
        "-c",
        "test \"$(id -u)\" = 10001 && /usr/local/bin/hello",
    ]);
    assert_success("compose run", &run);
    assert!(text(&run.stdout).contains("hello-card"));

    assert_success(
        "runtime check",
        &fixture.compose(&[
            "run",
            "--rm",
            "dev",
            "/run/dembly/bin/dembly",
            "__runtime",
            "check",
            "/run/dembly/runtime/dev.toml",
        ]),
    );

    let read_only = fixture.compose(&[
        "run",
        "--rm",
        "dev",
        "/bin/sh",
        "-c",
        "printf changed > /work/input",
    ]);
    assert!(
        !read_only.status.success(),
        "read-only Host Bind was writable"
    );
    assert_eq!(
        fs::read_to_string(fixture.deck_root.join("host-input")).unwrap(),
        "host-input\n"
    );
    assert_success(
        "read-write Host Bind",
        &fixture.compose(&[
            "run",
            "--rm",
            "dev",
            "/bin/sh",
            "-c",
            "printf retained > /work/rw/result",
        ]),
    );
    assert_eq!(
        fs::read_to_string(fixture.deck_root.join("host-rw/result")).unwrap(),
        "retained"
    );
    assert_eq!(
        fs::read_to_string(fixture.deck_root.join("volumes/cache/result")).unwrap(),
        "persisted\npersisted\npersisted\npersisted\npersisted\npersisted\n"
    );
    assert!(!fs::read_to_string("/proc/self/mountinfo")
        .unwrap()
        .contains("/opt/dembly/cards/hello"));

    let filesystem = fixture.card_root.join("rootfs.squashfs");
    let original_filesystem = fs::read(&filesystem).unwrap();
    let mut tampered = original_filesystem.clone();
    tampered.push(0);
    fs::write(&filesystem, tampered).unwrap();
    let checksum = fixture.dembly(&binary, &["lock"]);
    assert!(!checksum.status.success());
    assert!(text(&checksum.stderr).contains("checksum"));
    fs::write(&filesystem, original_filesystem).unwrap();

    fixture.replace_card(
        "exec = \"setup/post-mount.sh\"",
        "exec = \"setup/missing-post-mount.sh\"",
    );
    assert_success("failing dembly lock", &fixture.dembly(&binary, &["lock"]));
    assert_success("failing dembly apply", &fixture.dembly(&binary, &["apply"]));
    assert_success("failing compose up", &fixture.compose(&["up", "-d"]));
    let (status, exit_code) = fixture.wait_for_dev_exit();
    assert_eq!(status, "exited");
    assert_ne!(exit_code, "0");
    let failed_logs = fixture.compose(&["logs", "dev"]);
    assert_success("failed compose logs", &failed_logs);
    let failed_logs = text(&failed_logs.stdout);
    assert!(
        failed_logs.contains("post_mount hook"),
        "logs={failed_logs}"
    );
    assert!(
        failed_logs.contains("missing-post-mount.sh"),
        "logs={failed_logs}"
    );
    let failed_run = fixture.compose(&["run", "--rm", "dev", "/bin/true"]);
    assert!(!failed_run.status.success());
    fixture.assert_user_fields_retained(&original_sidecar);

    assert_success("compose down", &fixture.compose(&["down"]));
    let stopped = fixture.compose(&["ps", "-a", "-q"]);
    assert_success("stopped compose ps", &stopped);
    assert!(stopped.stdout.is_empty(), "ps={}", text(&stopped.stdout));
}

struct Fixture {
    root: PathBuf,
    deck_root: PathBuf,
    card_root: PathBuf,
    compose_file: PathBuf,
    devcontainer_file: PathBuf,
    image: String,
    project: String,
}

impl Fixture {
    fn new() -> Self {
        let repository = repository();
        let root =
            std::env::temp_dir().join(format!("dembly-compose-workflow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let deck_root = root.join(".dembly");
        let card_root = deck_root.join("cards/hello");
        fs::create_dir_all(root.join(".devcontainer")).unwrap();
        fs::create_dir_all(root.join("workspace")).unwrap();
        fs::create_dir_all(root.join("sidecar-data")).unwrap();
        let host_rw = deck_root.join("host-rw");
        fs::create_dir_all(&host_rw).unwrap();
        let mut permissions = fs::metadata(&host_rw).unwrap().permissions();
        permissions.set_mode(0o777);
        fs::set_permissions(&host_rw, permissions).unwrap();
        fs::write(deck_root.join("host-input"), "host-input\n").unwrap();

        let project = format!("dembly-acceptance-{}", std::process::id());
        let image = format!("dembly-nonroot-{}", std::process::id());
        let compose_file = root.join("compose.yaml");
        let compose =
            fs::read_to_string(repository.join("tests/fixtures/compose-workflow/compose.yaml"))
                .unwrap()
                .replace("__PROJECT_NAME__", &project)
                .replace("__IMAGE_NAME__", &image);
        fs::write(&compose_file, compose).unwrap();

        let devcontainer_file = root.join(".devcontainer/devcontainer.json");
        fs::copy(
            repository.join("tests/fixtures/compose-workflow/.devcontainer/devcontainer.json"),
            &devcontainer_file,
        )
        .unwrap();
        let initialize = root.join(".devcontainer/initialize-host.sh");
        fs::copy(
            repository.join("tests/fixtures/compose-workflow/.devcontainer/initialize-host.sh"),
            &initialize,
        )
        .unwrap();
        let mut permissions = fs::metadata(&initialize).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(initialize, permissions).unwrap();

        Self {
            root,
            deck_root,
            card_root,
            compose_file,
            devcontainer_file,
            image,
            project,
        }
    }

    fn build_image(&self) {
        assert_success(
            "docker build",
            &Command::new("docker")
                .args(["build", "-t", &self.image])
                .arg(repository().join("tests/fixtures/nonroot-base"))
                .output()
                .unwrap(),
        );
    }

    fn build_card(&self, binary: &Path) {
        assert_success(
            "card build",
            &Command::new(binary)
                .args(["card", "build"])
                .arg(repository().join("tests/fixtures/hello-card/rootfs"))
                .arg(self.deck_root.join("cards"))
                .args([
                    "--name",
                    "hello",
                    "--version",
                    "1",
                    "--mount-target",
                    "/opt/dembly/cards/hello",
                    "--path-prepend",
                    "bin",
                    "--non-interactive",
                ])
                .output()
                .unwrap(),
        );
        let manifest = self.card_root.join("card.toml");
        let source = fs::read_to_string(&manifest).unwrap();
        fs::write(
            manifest,
            format!(
                "{source}\n[environment]\nHELLO_ENV = \"enabled\"\n\n[[exports]]\nsource = \"bin/hello\"\ntarget = \"/usr/local/bin/hello\"\n\n[[hooks.post_mount]]\nexec = \"setup/post-mount.sh\"\n\n[check]\nexec = \"bin/hello\"\n"
            ),
        )
        .unwrap();
    }

    fn write_config(&self) {
        fs::write(
            self.deck_root.join("config.toml"),
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[devcontainer]\npath = \"../.devcontainer/devcontainer.json\"\n[[cards]]\npath = \"cards/hello/card.toml\"\n[environment]\nDECK_ENV = \"enabled\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/work/cache\"\n[[binds]]\nsource = \"${DECK_ROOT}/host-input\"\ntarget = \"/work/input\"\nmode = \"ro\"\n[[binds]]\nsource = \"${DECK_ROOT}/host-rw\"\ntarget = \"/work/rw\"\nmode = \"rw\"\n",
        )
        .unwrap();
    }

    fn dembly(&self, binary: &Path, arguments: &[&str]) -> Output {
        Command::new(binary)
            .current_dir(&self.root)
            .args(arguments)
            .output()
            .unwrap()
    }

    fn compose(&self, arguments: &[&str]) -> Output {
        Command::new("docker")
            .current_dir(&self.root)
            .args(["compose", "-f"])
            .arg(&self.compose_file)
            .args(arguments)
            .output()
            .unwrap()
    }

    fn selected_container(&self) -> String {
        let output = self.compose(&["ps", "-a", "-q", "dev"]);
        assert_success("compose selected container", &output);
        let id = text(&output.stdout).trim().to_owned();
        assert!(!id.is_empty(), "selected service has no container");
        let project = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{ index .Config.Labels \"com.docker.compose.project\" }}",
                &id,
            ])
            .output()
            .unwrap();
        assert_success("container project label", &project);
        assert_eq!(text(&project.stdout).trim(), self.project);
        id
    }

    fn image_id(&self) -> String {
        let output = Command::new("docker")
            .args(["image", "inspect", "--format", "{{.Id}}", &self.image])
            .output()
            .unwrap();
        assert_success("image inspect", &output);
        text(&output.stdout).trim().to_owned()
    }

    fn replace_card(&self, from: &str, to: &str) {
        let path = self.card_root.join("card.toml");
        let source = fs::read_to_string(&path).unwrap();
        assert!(source.contains(from), "manifest={source}");
        fs::write(path, source.replacen(from, to, 1)).unwrap();
    }

    fn document(&self) -> Value {
        serde_yaml::from_slice(&fs::read(&self.compose_file).unwrap()).unwrap()
    }

    fn service(&self, name: &str) -> Value {
        self.document()["services"][name].clone()
    }

    fn assert_user_fields_retained(&self, original_sidecar: &Value) {
        assert_eq!(&self.service("sidecar"), original_sidecar);
        let dev = self.service("dev");
        assert_eq!(dev["environment"]["COMPOSE_ENV"], "retained");
        assert!(dev["volumes"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|mount| mount
                .as_str()
                .is_some_and(|mount| mount.contains(":/workspace:rw"))));
    }

    fn assert_devcontainer_configuration(&self) {
        let output = Command::new("devcontainer")
            .args(["read-configuration", "--workspace-folder"])
            .arg(&self.root)
            .arg("--config")
            .arg(&self.devcontainer_file)
            .arg("--include-merged-configuration")
            .output()
            .unwrap();
        assert_success("devcontainer read-configuration", &output);
        let document: Value = serde_yaml::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "devcontainer JSON: {error}; stdout={}",
                text(&output.stdout)
            )
        });
        let configuration = &document["configuration"];
        assert_eq!(configuration["service"], "dev");
        assert_eq!(configuration["dockerComposeFile"][0], "../compose.yaml");
        assert_eq!(configuration["containerUser"], "root");
        assert_eq!(configuration["remoteUser"], "dembly");
    }

    fn wait_for_dev_exit(&self) -> (String, String) {
        for _ in 0..50 {
            let id = self.selected_container();
            let output = Command::new("docker")
                .args([
                    "inspect",
                    "--format",
                    "{{.State.Status}} {{.State.ExitCode}}",
                    &id,
                ])
                .output()
                .unwrap();
            assert_success("failed container inspect", &output);
            let state = text(&output.stdout);
            let mut fields = state.split_whitespace();
            let status = fields.next().unwrap().to_owned();
            let exit_code = fields.next().unwrap().to_owned();
            if status == "exited" {
                return (status, exit_code);
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("dev container did not exit after Runtime initialization failure");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.compose(&["down", "--remove-orphans"]);
        let _ = Command::new("docker")
            .args(["image", "rm", "-f", &self.image])
            .output();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn release_binary() -> PathBuf {
    RELEASE_BINARY
        .get_or_init(|| {
            let repository = repository();
            assert_success(
                "release binary build",
                &Command::new("cargo")
                    .current_dir(&repository)
                    .args([
                        "build",
                        "--release",
                        "--target",
                        "x86_64-unknown-linux-musl",
                        "-p",
                        "dembly-cli",
                    ])
                    .output()
                    .unwrap(),
            );
            repository.join("target/x86_64-unknown-linux-musl/release/dembly")
        })
        .clone()
}

fn assert_success(operation: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{operation}: status={} stdout={} stderr={}",
        output.status,
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
