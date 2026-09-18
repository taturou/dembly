use dembly_docker::{ManagedCompose, ManagedFields, ManagedMount};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn lock_embeds_resolved_identities_and_is_idempotent() {
    let fixture = Fixture::new();
    let before = fixture.document();
    let state_before = fixture.managed().state().unwrap().unwrap();

    let first = fixture.run();

    assert_success(&first);
    let first_bytes = fs::read(&fixture.compose).unwrap();
    let managed = fixture.managed();
    let embedded = managed.lock().unwrap().unwrap();
    assert_eq!(embedded.compose_path, fixture.compose.display().to_string());
    assert_eq!(embedded.service, "dev");
    assert_eq!(embedded.image, "sha256:immutable-image");
    assert_eq!(embedded.cards.len(), 1);
    assert_eq!(embedded.cards[0].name, "tool");
    assert_eq!(embedded.cards[0].version, "1");
    assert_eq!(
        embedded.cards[0].manifest_sha256,
        "6af7282834223858cc9900b0bb423c08dd4e6491169882ebd7b7fd0a7196cfc2"
    );
    assert_eq!(
        embedded.cards[0].filesystem_sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(managed.state().unwrap(), Some(state_before));
    assert_eq!(without_lock(fixture.document()), without_lock(before));
    assert_eq!(
        text(&first.stdout),
        format!(
            "locked: {}\nimage: sha256:immutable-image\ncards:\n  - tool 1\n",
            fixture.compose.display()
        )
    );

    let second = fixture.run();

    assert_success(&second);
    assert_eq!(second.stdout, first.stdout);
    assert_eq!(fs::read(&fixture.compose).unwrap(), first_bytes);
    fixture.assert_read_only_docker_calls(2);
}

#[test]
fn lock_rejects_conflicting_applied_state_without_updating_the_file() {
    let fixture = Fixture::new();
    fixture.replace_compose("user: root", "user: externally-changed");
    let before = fs::read(&fixture.compose).unwrap();

    let output = fixture.run();

    assert_failure(&output, "field user conflicts");
    assert_eq!(fs::read(&fixture.compose).unwrap(), before);
    fixture.assert_read_only_docker_calls(1);
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    compose: PathBuf,
    card_manifest: PathBuf,
    docker_log: PathBuf,
    compose_json: PathBuf,
    image_json: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-lock-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("project");
        let deck_root = project.join("nested/.dembly");
        let card_root = deck_root.join("cards/tool");
        let bin = root.join("bin");
        fs::create_dir_all(&card_root).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();

        let fixture = Self {
            compose: project.join("compose.yaml"),
            card_manifest: card_root.join("card.toml"),
            docker_log: root.join("docker.log"),
            compose_json: root.join("compose.json"),
            image_json: root.join("image.json"),
            root,
            project,
            bin,
        };
        fixture.write_inputs(&deck_root, &card_root);
        fixture.install_docker();
        fixture
    }

    fn write_inputs(&self, deck_root: &Path, card_root: &Path) {
        fs::write(
            deck_root.join("config.toml"),
            "schema_version = 1\n[compose]\npath = \"../.././compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"./cards/tool/card.toml\"\n",
        )
        .unwrap();
        fs::write(card_root.join("rootfs.squashfs"), b"abc").unwrap();
        fs::write(
            &self.card_manifest,
            "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/tool\"\n",
        )
        .unwrap();
        fs::write(
            &self.compose,
            "name: fixture\nx-custom:\n  retained: [one, two]\nservices:\n  dev:\n    image: example/dev:latest\n    user: vscode:staff\n    environment:\n      USER_FIELD: retained\n",
        )
        .unwrap();
        let mut compose = ManagedCompose::read(&self.compose).unwrap();
        compose
            .apply(
                "dev",
                ManagedFields {
                    entrypoint: Value::Sequence(vec![Value::String("/runtime/dembly".into())]),
                    command: Value::Sequence(Vec::new()),
                    user: Value::String("root".into()),
                    privileged: Value::Bool(true),
                    labels: BTreeMap::from([(
                        "io.dembly.lock-digest".into(),
                        Value::String("sha256:previous".into()),
                    )]),
                    mounts: vec![ManagedMount {
                        target: "/run/dembly/runtime/dev.toml".into(),
                        value: Value::String(
                            "./runtime/dev.toml:/run/dembly/runtime/dev.toml:ro".into(),
                        ),
                    }],
                },
                "sha256:previous",
            )
            .unwrap();
        fs::write(&self.compose, compose.to_bytes().unwrap()).unwrap();
        fs::write(
            &self.compose_json,
            r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/runtime/dembly"],"command":[],"user":"root","environment":{"USER_FIELD":"retained"}}}}"#,
        )
        .unwrap();
        fs::write(
            &self.image_json,
            r#"[{"Id":"sha256:immutable-image","Config":{"Entrypoint":[],"Cmd":[],"Env":[],"User":"image-user"}}]"#,
        )
        .unwrap();
        fs::write(&self.docker_log, "").unwrap();
    }

    fn install_docker(&self) {
        let docker = self.bin.join("docker");
        fs::write(
            &docker,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$DEMBLY_DOCKER_LOG\"\nif [ \"$1\" = compose ]; then exec /bin/cat \"$DEMBLY_COMPOSE_JSON\"; fi\nif [ \"$1\" = image ] && [ \"$2\" = inspect ]; then exec /bin/cat \"$DEMBLY_IMAGE_JSON\"; fi\nexit 91\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&docker).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(docker, permissions).unwrap();
    }

    fn run(&self) -> Output {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        Command::new(env!("CARGO_BIN_EXE_dembly"))
            .current_dir(&self.project)
            .args(["lock", "--config", "nested/.dembly/config.toml"])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HOME", self.root.join("home"))
            .env("DEMBLY_DOCKER_LOG", &self.docker_log)
            .env("DEMBLY_COMPOSE_JSON", &self.compose_json)
            .env("DEMBLY_IMAGE_JSON", &self.image_json)
            .output()
            .expect("dembly should start")
    }

    fn managed(&self) -> ManagedCompose {
        ManagedCompose::read(&self.compose).unwrap()
    }

    fn document(&self) -> Value {
        serde_yaml::from_slice(&fs::read(&self.compose).unwrap()).unwrap()
    }

    fn replace_compose(&self, from: &str, to: &str) {
        let source = fs::read_to_string(&self.compose).unwrap();
        assert!(source.contains(from), "{source}");
        fs::write(&self.compose, source.replacen(from, to, 1)).unwrap();
    }

    fn assert_read_only_docker_calls(&self, repetitions: usize) {
        let one = format!(
            "compose\n-f\n{}\nconfig\n--format\njson\nimage\ninspect\nexample/dev:latest\n",
            self.compose.display()
        );
        assert_eq!(
            fs::read_to_string(&self.docker_log).unwrap(),
            one.repeat(repetitions)
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn without_lock(mut document: Value) -> Value {
    if let Some(extension) = document
        .as_mapping_mut()
        .and_then(|root| root.get_mut(Value::String("x-dembly".into())))
        .and_then(Value::as_mapping_mut)
    {
        extension.remove(Value::String("lock".into()));
    }
    document
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
    String::from_utf8(bytes.to_vec()).unwrap()
}
