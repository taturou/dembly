use dembly_runtime::load_runtime_config;
use serde_yaml::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn apply_stages_runtime_artifacts_and_updates_only_the_selected_service() {
    let fixture = Fixture::new();
    fixture.lock();

    let output = fixture.run("apply");

    assert_success(&output);
    let runtime_root = fixture.deck_root.join("runtime");
    let runtime = load_runtime_config(&runtime_root.join("dev.toml")).unwrap();
    assert_eq!(runtime.runtime_user.spec, "vscode:staff");
    assert_eq!(runtime.process_argv, ["/image-init", "serve"]);
    assert_eq!(runtime.cards[0].name, "tool");
    assert_eq!(
        runtime.cards[0].image,
        Path::new("/run/dembly/cards/tool.squashfs")
    );
    assert_eq!(runtime.binds[0].source, Path::new("/run/dembly/binds/0"));
    assert_eq!(runtime.exports[0].source, Path::new("/opt/tool/bin/tool"));
    assert_eq!(runtime.hooks[0].exec, Path::new("/opt/tool/hooks/setup"));
    assert_eq!(runtime.checks[0].exec, Path::new("/opt/tool/bin/check"));
    assert_eq!(
        runtime.environment.get("CARD").map(String::as_str),
        Some("enabled")
    );

    let binary = runtime_root.join("bin/dembly");
    assert!(fs::metadata(&binary).unwrap().permissions().mode() & 0o111 != 0);
    assert_eq!(
        fs::read(&binary).unwrap(),
        fs::read(env!("CARGO_BIN_EXE_dembly")).unwrap()
    );
    assert!(fixture.deck_root.join("volumes/cache").is_dir());
    assert!(fixture.deck_root.join("volumes/tool/private").is_dir());
    assert!(fixture.deck_root.join("volumes/shared").is_dir());

    let ignore = fs::read_to_string(fixture.project.join(".gitignore")).unwrap();
    assert_eq!(ignore.matches("/.dembly/runtime/").count(), 1);
    assert_eq!(ignore.matches("/.dembly/volumes/").count(), 1);

    let document: Value = serde_yaml::from_slice(&fs::read(&fixture.compose).unwrap()).unwrap();
    let services = document["services"].as_mapping().unwrap();
    let dev = services["dev"].as_mapping().unwrap();
    assert_eq!(dev["entrypoint"].as_sequence().unwrap().len(), 4);
    assert_eq!(dev["command"], Value::Sequence(Vec::new()));
    assert_eq!(dev["user"], Value::String("root".into()));
    assert_eq!(dev["privileged"], Value::Bool(true));
    assert_eq!(dev["environment"]["USER_FIELD"], "retained");
    assert_eq!(services["other"]["command"], "untouched");
    let mounts = dev["volumes"].as_sequence().unwrap();
    let rendered = mounts
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(rendered[0].contains(":/run/dembly/bin/dembly:ro"));
    assert!(rendered[1].contains(":/run/dembly/runtime/dev.toml:ro"));
    assert!(rendered[2].contains(":/run/dembly/cards/tool.squashfs:ro"));
    assert!(rendered[3].contains(":/cache:rw"));
    assert!(rendered[4].contains(":/private:rw"));
    assert!(rendered[5].contains(":/shared:rw"));
    assert!(rendered[6].contains(":/run/dembly/binds/0:ro"));
}

#[test]
fn apply_requires_a_fresh_lock_before_writing_and_reapply_is_stable() {
    let fixture = Fixture::new();
    let original_compose = fs::read(&fixture.compose).unwrap();

    let missing = fixture.run("apply");

    assert_failure(&missing, "Lock is missing");
    assert_eq!(fs::read(&fixture.compose).unwrap(), original_compose);
    assert!(!fixture.deck_root.join("runtime").exists());

    fixture.lock();
    assert_success(&fixture.run("apply"));
    let first_compose = fs::read(&fixture.compose).unwrap();
    let first_runtime = fs::read(fixture.deck_root.join("runtime/dev.toml")).unwrap();
    let first_ignore = fs::read(fixture.project.join(".gitignore")).unwrap();

    assert_success(&fixture.run("apply"));
    assert_eq!(fs::read(&fixture.compose).unwrap(), first_compose);
    assert_eq!(
        fs::read(fixture.deck_root.join("runtime/dev.toml")).unwrap(),
        first_runtime
    );
    assert_eq!(
        fs::read(fixture.project.join(".gitignore")).unwrap(),
        first_ignore
    );

    fixture.replace_card("version = \"1\"", "version = \"2\"");
    let stale = fixture.run("apply");
    assert_failure(&stale, "Lock is stale");
    assert_eq!(fs::read(&fixture.compose).unwrap(), first_compose);
}

#[test]
fn apply_rejects_external_managed_field_edits_without_artifact_writes() {
    let fixture = Fixture::new();
    fixture.lock();
    assert_success(&fixture.run("apply"));
    fixture.replace_compose("user: root", "user: external");
    let before_compose = fs::read(&fixture.compose).unwrap();
    let before_runtime = fs::read(fixture.deck_root.join("runtime/dev.toml")).unwrap();

    let output = fixture.run("apply");

    assert_failure(&output, "field user conflicts");
    assert_eq!(fs::read(&fixture.compose).unwrap(), before_compose);
    assert_eq!(
        fs::read(fixture.deck_root.join("runtime/dev.toml")).unwrap(),
        before_runtime
    );
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    deck_root: PathBuf,
    config: PathBuf,
    compose: PathBuf,
    card: PathBuf,
    bin: PathBuf,
    compose_json: PathBuf,
    image_json: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-apply-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("project");
        let deck_root = project.join(".dembly");
        let card_root = deck_root.join("cards/tool");
        let bin = root.join("bin");
        fs::create_dir_all(&card_root).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        fs::write(deck_root.join("bind.txt"), "bind").unwrap();
        let fixture = Self {
            config: deck_root.join("config.toml"),
            compose: project.join("compose.yaml"),
            card: card_root.join("card.toml"),
            compose_json: root.join("compose.json"),
            image_json: root.join("image.json"),
            root,
            project,
            deck_root,
            bin,
        };
        fixture.write_inputs(card_root);
        fixture.install_docker();
        fixture
    }

    fn write_inputs(&self, card_root: PathBuf) {
        fs::write(
            &self.config,
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"cards/tool/card.toml\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/cache\"\n[[binds]]\nsource = \"${DECK_ROOT}/bind.txt\"\ntarget = \"${HOME}/bind.txt\"\nmode = \"ro\"\n[environment]\nDECK = \"yes\"\n",
        )
        .unwrap();
        fs::write(card_root.join("rootfs.squashfs"), b"abc").unwrap();
        fs::write(
            &self.card,
            "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/tool\"\n[environment]\nCARD = \"enabled\"\n[[exports]]\nsource = \"bin/tool\"\ntarget = \"/usr/local/bin/tool\"\n[[volumes]]\nname = \"private\"\ntarget = \"/private\"\n[[volumes]]\nname = \"shared\"\ntarget = \"/shared\"\nshared = true\n[[hooks.post_mount]]\nexec = \"hooks/setup\"\nargs = [\"one\"]\n[check]\nexec = \"bin/check\"\nargs = [\"--quick\"]\n",
        )
        .unwrap();
        fs::write(
            &self.compose,
            "name: fixture\nservices:\n  dev:\n    image: example/dev:latest\n    command: [serve]\n    user: vscode:staff\n    environment:\n      USER_FIELD: retained\n  other:\n    image: example/other:latest\n    command: untouched\n",
        )
        .unwrap();
        fs::write(
            &self.compose_json,
            r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":null,"command":["serve"],"user":"vscode:staff","environment":{"USER_FIELD":"retained"}},"other":{"image":"example/other:latest","command":["untouched"]}}}"#,
        )
        .unwrap();
        fs::write(
            &self.image_json,
            r#"[{"Id":"sha256:image","Config":{"Entrypoint":["/image-init"],"Cmd":["default"],"Env":["PATH=/usr/bin"],"User":"image-user"}}]"#,
        )
        .unwrap();
        fs::write(self.project.join(".gitignore"), "target/\n").unwrap();
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

    fn lock(&self) {
        assert_success(&self.run("lock"));
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

    fn replace_card(&self, from: &str, to: &str) {
        let source = fs::read_to_string(&self.card).unwrap();
        fs::write(&self.card, source.replacen(from, to, 1)).unwrap();
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
