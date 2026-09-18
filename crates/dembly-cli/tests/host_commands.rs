use dembly_core::{sha256_bytes, sha256_file};
use dembly_docker::{DemblyLock, ManagedCompose, ManagedFields, ManagedMount, ManagedValue};
use dembly_runtime::{render_runtime_config, RuntimeCard, RuntimeConfig, RuntimeUserSpec};
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn validate_resolves_explicit_config_without_writing_project_files() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();

    let output = fixture.run(["validate", "--config", ".dembly/config.toml"]);

    assert_success(&output);
    assert!(text(&output.stdout).contains("valid:"));
    assert_eq!(fixture.snapshot(), before);
    fixture.assert_read_only_docker_calls();
}

#[test]
fn validate_reports_a_missing_config_without_writing_project_files() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();

    let output = fixture.run(["validate", "--config", "missing.toml"]);

    assert_failure(&output, "cannot resolve config path");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn validate_warns_about_an_optional_missing_bind_without_writing_project_files() {
    let fixture = Fixture::new();
    fixture.add_optional_missing_bind();
    let before = fixture.snapshot();

    let output = fixture.run(["validate"]);

    assert_success(&output);
    assert!(text(&output.stderr).contains("optional Host Bind"));
    assert_eq!(fixture.snapshot(), before);
    fixture.assert_read_only_docker_calls();
}

#[test]
fn validate_rejects_external_changes_to_every_managed_field_without_writing_project_files() {
    for field in ["user", "privileged", "label", "mount"] {
        let fixture = Fixture::new();
        fixture.write_lock(false);
        let digest = fixture.current_lock_digest();
        fixture.write_applied_state(&digest);
        fixture.mutate_managed_field(field);
        let before = fixture.snapshot();

        let output = fixture.run(["validate"]);

        assert_failure(&output, "conflicts");
        assert_eq!(fixture.snapshot(), before, "field={field}");
        fixture.assert_read_only_docker_calls();
    }
}

#[test]
fn inspect_renders_the_resolved_host_state_without_writing_project_files() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();

    let output = fixture.run(["inspect"]);

    assert_success(&output);
    assert_eq!(
        text(&output.stdout),
        format!(
            "config: {}\ndeck root: {}\ncompose:\n  project: fixture\n  path: {}\n  service: dev\nuser: vscode:staff\ncards:\n  - tool 1\n    manifest: {}\n    filesystem: {}\n    mount: /opt/tool\nmounts:\n  - card:tool -> /opt/tool\nenvironment:\n  CARD=enabled\n  COMPOSE=only\n  MODE=deck\n  PATH=/opt/tool/bin:/deck/bin:/usr/bin\nlock: missing\napply: not applied\nconflicts: none\nnext apply:\n  fields:\n    entrypoint: /run/dembly/bin/dembly __runtime init /run/dembly/runtime/dev.toml\n    command: []\n    user: root\n    privileged: true\n    label: io.dembly.lock-digest\n  artifacts:\n    - {}/runtime/dev.toml\n    - {}/runtime/bin/dembly\n",
            fixture.config.display(),
            fixture.config.parent().unwrap().display(),
            fixture.compose.display(),
            fixture.card_manifest.display(),
            fixture.filesystem.display(),
            fixture.config.parent().unwrap().display(),
            fixture.config.parent().unwrap().display(),
        )
    );
    assert_eq!(fixture.snapshot(), before);
    fixture.assert_read_only_docker_calls();
}

#[test]
fn check_rejects_a_missing_lock_without_writing_project_files() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "dembly lock");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_rejects_a_stale_lock_without_writing_project_files() {
    let fixture = Fixture::new();
    fixture.write_lock(true);
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "dembly lock");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_requires_an_applied_state_without_writing_project_files() {
    let fixture = Fixture::new();
    fixture.write_lock(false);
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "applied state");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_reports_a_managed_field_conflict_without_writing_project_files() {
    let fixture = Fixture::new();
    fixture.write_lock(false);
    let digest = fixture.current_lock_digest();
    fixture.write_applied_state(&digest);
    fixture.replace_compose("user: root", "user: altered");
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "field user conflicts");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_verifies_static_host_integrity_without_running_card_checks() {
    let fixture = Fixture::new();
    fixture.write_lock(false);
    let digest = fixture.current_lock_digest();
    fixture.write_applied_state(&digest);
    fixture.write_runtime_artifacts(&digest);
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_success(&output);
    assert!(text(&output.stdout).contains("host integrity: valid"));
    assert!(text(&output.stdout).contains(&format!(
        "Run Card checks with: docker compose -f {} run --rm dev /run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml",
        fixture.compose.display()
    )));
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_rejects_tampering_in_every_runtime_plan_section() {
    for (from, to) in [
        ("spec = \"vscode:staff\"", "spec = \"altered\""),
        ("name = \"tool\"", "name = \"altered\""),
        ("rootfs.squashfs", "altered.squashfs"),
        (
            "mount_target = \"/opt/tool\"",
            "mount_target = \"/opt/altered\"",
        ),
        (
            "volume_targets = [\"/cache\"]",
            "volume_targets = [\"/altered\"]",
        ),
        ("${HOME}/.config", "${HOME}/.tampered"),
        ("/opt/tool/bin/tool", "/opt/tool/bin/altered"),
        ("/opt/tool/setup", "/opt/tool/altered-setup"),
        (
            "exec = \"/opt/tool/check\"",
            "exec = \"/opt/tool/altered-check\"",
        ),
        ("args = [\"--version\"]", "args = [\"--tampered\"]"),
        ("MODE = \"runtime\"", "MODE = \"tampered\""),
        ("argv = [\"serve\"]", "argv = [\"tampered\"]"),
    ] {
        let fixture = Fixture::new();
        fixture.write_lock(false);
        let digest = fixture.current_lock_digest();
        fixture.write_applied_state(&digest);
        fixture.write_runtime_artifacts(&digest);
        let runtime = fixture.project.join(".dembly/runtime/dev.toml");
        let source = fs::read_to_string(&runtime).unwrap();
        assert!(source.contains(from), "missing fixture fragment {from:?}");
        fs::write(&runtime, source.replacen(from, to, 1)).unwrap();
        let before = fixture.snapshot();

        let output = fixture.run(["check"]);

        assert_failure(&output, "Runtime plan digest");
        assert_eq!(fixture.snapshot(), before);
    }
}

#[test]
fn check_rejects_runtime_binary_byte_tampering() {
    let fixture = Fixture::new();
    fixture.write_lock(false);
    let digest = fixture.current_lock_digest();
    fixture.write_applied_state(&digest);
    fixture.write_runtime_artifacts(&digest);
    fs::write(
        fixture.project.join(".dembly/runtime/bin/dembly"),
        b"tampered executable",
    )
    .unwrap();
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "Runtime binary digest");
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn check_succeeds_without_invoking_docker() {
    let fixture = Fixture::new();
    fixture.write_lock(false);
    let digest = fixture.current_lock_digest();
    fixture.write_applied_state(&digest);
    fixture.write_runtime_artifacts(&digest);
    fs::write(
        fixture.bin.join("docker"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$DEMBLY_DOCKER_LOG\"\nexit 97\n",
    )
    .unwrap();
    fs::write(&fixture.docker_log, "").unwrap();

    let output = fixture.run(["check"]);

    assert_success(&output);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_rejects_runtime_state_from_a_previous_lock_generation_without_writing_project_files() {
    let fixture = Fixture::new();
    fixture.write_lock(true);
    let old_digest = fixture.current_lock_digest();
    fixture.write_applied_state(&old_digest);
    fixture.write_runtime_artifacts(&old_digest);
    fixture.write_lock(false);
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "lock digest");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

#[test]
fn check_rejects_a_managed_lock_label_from_a_previous_generation_without_writing_project_files() {
    let fixture = Fixture::new();
    fixture.write_lock(false);
    let digest = fixture.current_lock_digest();
    fixture.write_applied_state(&digest);
    fixture.write_runtime_artifacts(&digest);
    fixture.replace_applied_lock_label("sha256:previous");
    let before = fixture.snapshot();

    let output = fixture.run(["check"]);

    assert_failure(&output, "managed label");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.docker_log(), "");
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    config: PathBuf,
    compose: PathBuf,
    card_manifest: PathBuf,
    filesystem: PathBuf,
    docker_log: PathBuf,
    bin: PathBuf,
    compose_json: PathBuf,
    image_json: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-host-commands-{}-{}",
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

        let fixture = Self {
            config: deck_root.join("config.toml"),
            compose: project.join("compose.yaml"),
            card_manifest: card_root.join("card.toml"),
            filesystem: card_root.join("rootfs.squashfs"),
            docker_log: root.join("docker.log"),
            compose_json: root.join("compose.json"),
            image_json: root.join("image.json"),
            root,
            project,
            bin,
        };
        fixture.write_inputs();
        fixture.install_docker();
        fixture
    }

    fn write_inputs(&self) {
        fs::write(
            &self.config,
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"cards/tool/card.toml\"\n[environment]\nMODE = \"deck\"\n[environment_path]\nprepend = [\"/deck/bin\"]\n",
        )
        .unwrap();
        fs::write(&self.filesystem, b"abc").unwrap();
        fs::write(
            &self.card_manifest,
            "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/tool\"\n[environment]\nCARD = \"enabled\"\n[environment_path]\nprepend = [\"bin\"]\n[check]\nexec = \"bin/never-run\"\n",
        )
        .unwrap();
        fs::write(
            &self.compose,
            "name: fixture\nservices:\n  dev:\n    image: example/dev:latest\n    user: vscode:staff\n",
        )
        .unwrap();
        fs::write(
            &self.compose_json,
            r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/init"],"command":["serve"],"user":"vscode:staff","environment":{"COMPOSE":"only"}}}}"#,
        )
        .unwrap();
        fs::write(
            &self.image_json,
            r#"[{"Id":"sha256:image","Config":{"Entrypoint":[],"Cmd":[],"Env":["PATH=/usr/bin","MODE=image"],"User":"image-user"}}]"#,
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

    fn run<const N: usize>(&self, arguments: [&str; N]) -> Output {
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        Command::new(env!("CARGO_BIN_EXE_dembly"))
            .current_dir(&self.project)
            .args(arguments)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HOME", self.root.join("home"))
            .env("DEMBLY_DOCKER_LOG", &self.docker_log)
            .env("DEMBLY_COMPOSE_JSON", &self.compose_json)
            .env("DEMBLY_IMAGE_JSON", &self.image_json)
            .output()
            .expect("dembly should start")
    }

    fn add_optional_missing_bind(&self) {
        let mut config = fs::read_to_string(&self.config).unwrap();
        config.push_str("\n[[binds]]\nsource = \"${DECK_ROOT}/optional\"\ntarget = \"/work/optional\"\nmode = \"ro\"\nrequired = false\n");
        fs::write(&self.config, config).unwrap();
    }

    fn write_lock(&self, stale: bool) {
        let mut compose = ManagedCompose::read(&self.compose).unwrap();
        compose
            .set_lock(DemblyLock {
                compose_path: self.compose.display().to_string(),
                service: "dev".into(),
                image: "sha256:image".into(),
                cards: vec![dembly_docker::CardLock {
                    name: "tool".into(),
                    version: if stale { "stale" } else { "1" }.into(),
                    manifest_sha256: sha256_file(&self.card_manifest).unwrap(),
                    filesystem_sha256:
                        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
                }],
            })
            .unwrap();
        fs::write(&self.compose, compose.to_bytes().unwrap()).unwrap();
    }

    fn current_lock_digest(&self) -> String {
        let lock = ManagedCompose::read(&self.compose)
            .unwrap()
            .lock()
            .unwrap()
            .unwrap();
        canonical_lock_digest(&lock)
    }

    fn write_applied_state(&self, lock_digest: &str) {
        let (runtime_bytes, binary_bytes) = self.runtime_artifact_contents(lock_digest);
        let mut compose = ManagedCompose::read(&self.compose).unwrap();
        compose
            .apply(
                "dev",
                ManagedFields {
                    entrypoint: Value::Sequence(vec![Value::String("/runtime/dembly".into())]),
                    command: Value::Sequence(Vec::new()),
                    user: Value::String("root".into()),
                    privileged: Value::Bool(true),
                    labels: BTreeMap::from([
                        (
                            "io.dembly.lock-digest".into(),
                            Value::String(lock_digest.into()),
                        ),
                        (
                            "io.dembly.runtime-plan-digest".into(),
                            Value::String(format!("sha256:{}", sha256_bytes(&runtime_bytes))),
                        ),
                        (
                            "io.dembly.runtime-binary-digest".into(),
                            Value::String(format!("sha256:{}", sha256_bytes(&binary_bytes))),
                        ),
                    ]),
                    mounts: vec![ManagedMount {
                        target: "/run/dembly/runtime/dev.toml".into(),
                        value: Value::String(
                            "./runtime/dev.toml:/run/dembly/runtime/dev.toml:ro".into(),
                        ),
                    }],
                },
                lock_digest,
            )
            .unwrap();
        fs::write(&self.compose, compose.to_bytes().unwrap()).unwrap();
        fs::write(
            &self.compose_json,
            r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/runtime/dembly"],"command":[],"user":"root","environment":{"COMPOSE":"only"}}}}"#,
        )
        .unwrap();
    }

    fn write_runtime_artifacts(&self, lock_digest: &str) {
        let runtime = self.project.join(".dembly/runtime");
        fs::create_dir_all(runtime.join("bin")).unwrap();
        let (runtime_bytes, binary_bytes) = self.runtime_artifact_contents(lock_digest);
        fs::write(runtime.join("dev.toml"), runtime_bytes).unwrap();
        let binary = runtime.join("bin/dembly");
        fs::write(&binary, binary_bytes).unwrap();
        let mut permissions = fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(binary, permissions).unwrap();
    }

    fn runtime_artifact_contents(&self, lock_digest: &str) -> (Vec<u8>, Vec<u8>) {
        let config = RuntimeConfig {
            schema_version: 1,
            lock_digest: lock_digest.into(),
            runtime_user: RuntimeUserSpec {
                spec: "vscode:staff".into(),
            },
            cards: vec![RuntimeCard {
                name: "tool".into(),
                image: self.filesystem.clone(),
                mount_target: PathBuf::from("/opt/tool"),
            }],
            volume_targets: vec![PathBuf::from("/cache")],
            binds: vec![dembly_runtime::RuntimeBind {
                source: PathBuf::from("/run/dembly/binds/0"),
                target: "${HOME}/.config".into(),
                mode: "ro".into(),
            }],
            exports: vec![dembly_runtime::RuntimeExport {
                source: PathBuf::from("/opt/tool/bin/tool"),
                target: PathBuf::from("/usr/local/bin/tool"),
            }],
            hooks: vec![dembly_runtime::RuntimeHook {
                card: "tool".into(),
                exec: PathBuf::from("/opt/tool/setup"),
                args: vec!["--install".into()],
            }],
            checks: vec![dembly_runtime::RuntimeCheck {
                card: "tool".into(),
                exec: PathBuf::from("/opt/tool/check"),
                args: vec!["--version".into()],
            }],
            environment: BTreeMap::from([("MODE".into(), "runtime".into())]),
            process_argv: vec!["serve".into()],
        };
        (
            render_runtime_config(&config).unwrap().into_bytes(),
            b"runtime binary".to_vec(),
        )
    }

    fn replace_compose(&self, from: &str, to: &str) {
        let compose = fs::read_to_string(&self.compose).unwrap();
        assert!(compose.contains(from), "{compose}");
        fs::write(&self.compose, compose.replacen(from, to, 1)).unwrap();
    }

    fn mutate_managed_field(&self, field: &str) {
        let source = fs::read_to_string(&self.compose).unwrap();
        let mut document = serde_yaml::from_str::<Value>(&source).unwrap();
        let service = document
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("services".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("dev".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap();
        match field {
            "user" => {
                service.insert(
                    Value::String("user".into()),
                    Value::String("altered".into()),
                );
            }
            "privileged" => {
                service.insert(Value::String("privileged".into()), Value::Bool(false));
            }
            "label" => {
                service
                    .get_mut(Value::String("labels".into()))
                    .unwrap()
                    .as_mapping_mut()
                    .unwrap()
                    .insert(
                        Value::String("io.dembly.lock-digest".into()),
                        Value::String("sha256:altered".into()),
                    );
            }
            "mount" => {
                service
                    .get_mut(Value::String("volumes".into()))
                    .unwrap()
                    .as_sequence_mut()
                    .unwrap()[0] =
                    Value::String("./runtime/other.toml:/run/dembly/runtime/dev.toml:ro".into());
            }
            other => panic!("unknown managed field {other}"),
        }
        fs::write(&self.compose, serde_yaml::to_string(&document).unwrap()).unwrap();
    }

    fn replace_applied_lock_label(&self, digest: &str) {
        let source = fs::read_to_string(&self.compose).unwrap();
        let mut document = serde_yaml::from_str::<Value>(&source).unwrap();
        let root = document.as_mapping_mut().unwrap();
        let service = root
            .get_mut(Value::String("services".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("dev".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap();
        service
            .get_mut(Value::String("labels".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .insert(
                Value::String("io.dembly.lock-digest".into()),
                Value::String(digest.into()),
            );
        let entry = root
            .get_mut(Value::String("x-dembly".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("state".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("fields".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("labels".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("entries".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap()
            .get_mut(Value::String("io.dembly.lock-digest".into()))
            .unwrap()
            .as_mapping_mut()
            .unwrap();
        entry.insert(
            Value::String("applied".into()),
            serde_yaml::to_value(ManagedValue::Present(Value::String(digest.into()))).unwrap(),
        );
        fs::write(&self.compose, serde_yaml::to_string(&document).unwrap()).unwrap();
    }

    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut paths = BTreeSet::new();
        collect_files(&self.project, &self.project, &mut paths);
        paths
            .into_iter()
            .map(|path| {
                let relative = path.strip_prefix(&self.project).unwrap().to_path_buf();
                (relative, fs::read(path).unwrap())
            })
            .collect()
    }

    fn docker_log(&self) -> String {
        fs::read_to_string(&self.docker_log).unwrap()
    }

    fn assert_read_only_docker_calls(&self) {
        assert_eq!(
            self.docker_log(),
            format!(
                "compose\n-f\n{}\nconfig\n--format\njson\nimage\ninspect\nexample/dev:latest\n",
                self.compose.display()
            )
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn collect_files(root: &Path, directory: &Path, paths: &mut BTreeSet<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let metadata = fs::symlink_metadata(&path).unwrap();
        if metadata.file_type().is_dir() {
            collect_files(root, &path, paths);
        } else if metadata.file_type().is_file() {
            assert!(path.starts_with(root));
            paths.insert(path);
        }
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
    assert!(
        !output.status.success(),
        "stdout={} stderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert!(
        text(&output.stderr).contains(expected),
        "stderr={}",
        text(&output.stderr)
    );
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn canonical_lock_digest(lock: &DemblyLock) -> String {
    let mut content = b"dembly-lock-v1\0".to_vec();
    for value in [&lock.compose_path, &lock.service, &lock.image] {
        append_canonical_field(&mut content, value);
    }
    for card in &lock.cards {
        for value in [
            &card.name,
            &card.version,
            &card.manifest_sha256,
            &card.filesystem_sha256,
        ] {
            append_canonical_field(&mut content, value);
        }
    }
    let mut command = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write as _;
    command.stdin.as_mut().unwrap().write_all(&content).unwrap();
    let output = command.wait_with_output().unwrap();
    assert!(output.status.success());
    format!(
        "sha256:{}",
        String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
    )
}

fn append_canonical_field(content: &mut Vec<u8>, value: &str) {
    content.extend_from_slice(value.len().to_string().as_bytes());
    content.push(b':');
    content.extend_from_slice(value.as_bytes());
    content.push(0);
}
