use serde_yaml::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn unapply_restores_original_fields_and_retains_persistent_data() {
    let fixture = Fixture::new();
    fixture.lock_apply();
    fs::create_dir_all(fixture.deck_root.join("volumes")).unwrap();
    fs::write(fixture.deck_root.join("volumes/keep"), "volume-data").unwrap();
    fs::write(fixture.deck_root.join("keep.txt"), "config-data").unwrap();
    let ignore = fs::read(&fixture.ignore).unwrap();

    let output = fixture.run("unapply");

    assert_success(&output);
    let document: Value = serde_yaml::from_slice(&fs::read(&fixture.compose).unwrap()).unwrap();
    let service = &document["services"]["dev"];
    assert!(service.get("entrypoint").is_none());
    assert_eq!(service["command"], Value::Null);
    assert_eq!(service["user"], "vscode:staff");
    assert_eq!(service["privileged"], Value::Bool(false));
    assert_eq!(service["labels"]["user.label"], "kept");
    assert_eq!(service["volumes"][0], "./user:/user:ro");
    assert!(document["x-dembly"].get("lock").is_some());
    assert!(document["x-dembly"].get("state").is_none());
    assert!(!fixture.deck_root.join("runtime").exists());
    assert_eq!(
        fs::read(fixture.deck_root.join("volumes/keep")).unwrap(),
        b"volume-data"
    );
    assert_eq!(
        fs::read(fixture.deck_root.join("keep.txt")).unwrap(),
        b"config-data"
    );
    assert_eq!(fs::read(&fixture.ignore).unwrap(), ignore);

    assert_failure(&fixture.run("unapply"), "applied state");
}

#[test]
fn unapply_conflict_preserves_compose_and_runtime() {
    let fixture = Fixture::new();
    fixture.lock_apply();
    fixture.replace_compose("user: root", "user: external");
    let compose = fs::read(&fixture.compose).unwrap();

    let output = fixture.run("unapply");

    assert_failure(&output, "field user conflicts");
    assert_eq!(fs::read(&fixture.compose).unwrap(), compose);
    assert!(fixture.deck_root.join("runtime/dev.toml").is_file());
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    deck_root: PathBuf,
    compose: PathBuf,
    ignore: PathBuf,
    bin: PathBuf,
    compose_json: PathBuf,
    image_json: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-unapply-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("project");
        let deck_root = project.join(".dembly");
        let bin = root.join("bin");
        fs::create_dir_all(deck_root.join("cards/tool")).unwrap();
        fs::create_dir_all(project.join("user")).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        let fixture = Self {
            compose: project.join("compose.yaml"),
            ignore: project.join(".gitignore"),
            compose_json: root.join("compose.json"),
            image_json: root.join("image.json"),
            root,
            project,
            deck_root,
            bin,
        };
        fixture.write_inputs();
        fixture.install_docker();
        fixture
    }

    fn write_inputs(&self) {
        fs::write(
            self.deck_root.join("config.toml"),
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"cards/tool/card.toml\"\n",
        )
        .unwrap();
        fs::write(self.deck_root.join("cards/tool/rootfs.squashfs"), b"abc").unwrap();
        fs::write(
            self.deck_root.join("cards/tool/card.toml"),
            "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/tool\"\n",
        )
        .unwrap();
        fs::write(
            &self.compose,
            "name: fixture\nservices:\n  dev:\n    image: example/dev:latest\n    command: null\n    user: vscode:staff\n    privileged: false\n    labels:\n      user.label: kept\n    volumes:\n      - ./user:/user:ro\n",
        )
        .unwrap();
        fs::write(
            &self.compose_json,
            r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":null,"command":null,"user":"vscode:staff","environment":{}}}}"#,
        )
        .unwrap();
        fs::write(
            &self.image_json,
            r#"[{"Id":"sha256:image","Config":{"Entrypoint":["/init"],"Cmd":["serve"],"Env":[],"User":"image-user"}}]"#,
        )
        .unwrap();
        fs::write(&self.ignore, "target/\n").unwrap();
    }

    fn install_docker(&self) {
        let docker = self.bin.join("docker");
        fs::write(
            &docker,
            "#!/bin/sh\nif [ \"$1\" = compose ]; then exec /bin/cat \"$DEMBLY_COMPOSE_JSON\"; fi\nif [ \"$1\" = image ]; then exec /bin/cat \"$DEMBLY_IMAGE_JSON\"; fi\nexit 91\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&docker).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(docker, permissions).unwrap();
    }

    fn lock_apply(&self) {
        assert_success(&self.run("lock"));
        assert_success(&self.run("apply"));
    }

    fn run(&self, command: &str) -> Output {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        Command::new(env!("CARGO_BIN_EXE_dembly"))
            .current_dir(&self.project)
            .args([command, "--config", ".dembly/config.toml"])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HOME", self.root.join("home"))
            .env("DEMBLY_COMPOSE_JSON", &self.compose_json)
            .env("DEMBLY_IMAGE_JSON", &self.image_json)
            .output()
            .unwrap()
    }

    fn replace_compose(&self, from: &str, to: &str) {
        let source = fs::read_to_string(&self.compose).unwrap();
        assert!(source.contains(from), "{source}");
        fs::write(&self.compose, source.replacen(from, to, 1)).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &Output, expected: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
