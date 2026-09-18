use dembly_cli::HostPlan;
use dembly_docker::{ManagedCompose, ManagedFields};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static ENVIRONMENT: Mutex<()> = Mutex::new(());

#[test]
fn resolves_one_plan_through_read_only_docker_inspection() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();

    let plan = fixture.resolve().unwrap();

    assert_eq!(plan.deck.document.compose.service, "dev");
    assert_eq!(plan.managed_compose.project_name().unwrap(), "fixture");
    assert_eq!(
        plan.compose_files,
        vec![
            fixture.base_compose.clone(),
            fixture.managed_compose.clone()
        ]
    );
    assert_eq!(plan.effective_service.image, "example/dev:latest");
    assert_eq!(plan.intended_user_spec(), "vscode:staff");
    assert_eq!(
        plan.environment.get("PATH").unwrap(),
        "/opt/tool/bin:/deck/bin:/usr/bin"
    );
    assert_eq!(plan.environment.get("MODE").unwrap(), "deck");
    assert_eq!(plan.environment.get("CARD").unwrap(), "enabled");
    assert_eq!(plan.environment.get("COMPOSE").unwrap(), "only");
    assert!(!plan.environment.contains_key("REMOVED"));
    assert_eq!(
        fixture.log(),
        format!(
            "compose\n-f\n{}\n-f\n{}\nconfig\n--format\njson\nimage\ninspect\nexample/dev:latest\n",
            fixture.base_compose.display(),
            fixture.managed_compose.display()
        )
    );
}

#[test]
fn intended_user_falls_back_to_image_user_then_root() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    fixture.write_compose_json(
        r#"{"services":{"dev":{"image":"example/dev:latest","user":"","environment":{}}}}"#,
    );
    fixture.write_image_json("image-user:staff");
    fixture.write_devcontainer(
        "dev",
        &["../compose.base.yaml", "../compose.yaml"],
        "image-user",
    );
    assert_eq!(
        fixture.resolve().unwrap().intended_user_spec(),
        "image-user:staff"
    );

    fixture.clear_log();
    fixture.write_image_json("");
    fixture.write_devcontainer("dev", &["../compose.base.yaml", "../compose.yaml"], "root");
    assert_eq!(fixture.resolve().unwrap().intended_user_spec(), "root");
}

#[test]
fn applied_state_restores_pre_apply_process_and_user_before_devcontainer_validation() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    fs::write(
        &fixture.managed_compose,
        "name: fixture\nservices:\n  dev:\n    image: example/dev:latest\n    entrypoint: [/original-init]\n    command: [serve]\n    user: vscode:staff\n",
    )
    .unwrap();
    let mut compose = ManagedCompose::read(&fixture.managed_compose).unwrap();
    compose
        .apply(
            "dev",
            ManagedFields {
                entrypoint: Value::Sequence(vec![Value::String("/run/dembly/bin/dembly".into())]),
                command: Value::Sequence(Vec::new()),
                user: Value::String("root".into()),
                privileged: Value::Bool(true),
                labels: BTreeMap::new(),
                mounts: Vec::new(),
            },
            "sha256:applied",
        )
        .unwrap();
    fs::write(&fixture.managed_compose, compose.to_bytes().unwrap()).unwrap();
    fixture.write_compose_json(
        r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/run/dembly/bin/dembly"],"command":[],"user":"root","environment":{"MODE":"compose"}}}}"#,
    );

    let plan = fixture.resolve().unwrap();

    assert_eq!(plan.intended_user_spec(), "vscode:staff");
    assert_eq!(plan.effective_service.user.as_deref(), Some("vscode:staff"));
    assert_eq!(
        plan.effective_service.entrypoint,
        Some(vec!["/original-init".into()])
    );
    assert_eq!(plan.effective_service.command, Some(vec!["serve".into()]));
    assert_eq!(
        fixture.log(),
        format!(
            "compose\n-f\n{}\n-f\n{}\nconfig\n--format\njson\nimage\ninspect\nexample/dev:latest\n",
            fixture.base_compose.display(),
            fixture.managed_compose.display()
        )
    );
}

#[test]
fn applied_state_restores_originals_inherited_from_earlier_compose_files() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    let mut compose = ManagedCompose::read(&fixture.managed_compose).unwrap();
    compose
        .apply(
            "dev",
            ManagedFields {
                entrypoint: Value::Sequence(vec![Value::String("/run/dembly/bin/dembly".into())]),
                command: Value::Sequence(Vec::new()),
                user: Value::String("root".into()),
                privileged: Value::Bool(true),
                labels: BTreeMap::new(),
                mounts: Vec::new(),
            },
            "sha256:applied",
        )
        .unwrap();
    fs::write(&fixture.managed_compose, compose.to_bytes().unwrap()).unwrap();
    fixture.write_compose_json(
        r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/run/dembly/bin/dembly"],"command":[],"user":"root","environment":{"MODE":"compose"}}}}"#,
    );
    fixture.write_base_compose_json(
        r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/base-init"],"command":["serve"],"user":"base-user:staff","environment":{"MODE":"compose"}}}}"#,
    );
    fixture.write_devcontainer(
        "dev",
        &["../compose.base.yaml", "../compose.yaml"],
        "base-user",
    );

    let plan = fixture.resolve().unwrap();

    assert_eq!(plan.intended_user_spec(), "base-user:staff");
    assert_eq!(
        plan.effective_service.entrypoint,
        Some(vec!["/base-init".into()])
    );
    assert_eq!(plan.effective_service.command, Some(vec!["serve".into()]));
    assert_eq!(
        fixture.log(),
        format!(
            "compose\n-f\n{}\n-f\n{}\nconfig\n--format\njson\ncompose\n-f\n{}\nconfig\n--format\njson\nimage\ninspect\nexample/dev:latest\n",
            fixture.base_compose.display(),
            fixture.managed_compose.display(),
            fixture.base_compose.display()
        )
    );
}

#[test]
fn applied_state_without_resolvable_prefix_service_falls_back_to_image_defaults() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    let mut compose = ManagedCompose::read(&fixture.managed_compose).unwrap();
    compose
        .apply(
            "dev",
            ManagedFields {
                entrypoint: Value::Sequence(vec![Value::String("/run/dembly/bin/dembly".into())]),
                command: Value::Sequence(Vec::new()),
                user: Value::String("root".into()),
                privileged: Value::Bool(true),
                labels: BTreeMap::new(),
                mounts: Vec::new(),
            },
            "sha256:applied",
        )
        .unwrap();
    fs::write(&fixture.managed_compose, compose.to_bytes().unwrap()).unwrap();
    fixture.write_compose_json(
        r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/run/dembly/bin/dembly"],"command":[],"user":"root","environment":{}}}}"#,
    );
    fixture.write_devcontainer(
        "dev",
        &["../compose.base.yaml", "../compose.yaml"],
        "image-user",
    );

    for prefix in [
        r#"{"services":{"database":{"image":"postgres:17"}}}"#,
        r#"{"services":{"dev":{"user":"prefix-user"}}}"#,
    ] {
        fixture.clear_log();
        fixture.write_base_compose_json(prefix);

        let plan = fixture.resolve().unwrap();

        assert_eq!(plan.intended_user_spec(), "image-user");
        assert_eq!(plan.effective_service.entrypoint, None);
        assert_eq!(plan.effective_service.command, None);
        assert_eq!(plan.effective_service.user, None);
        assert_eq!(
            fixture.log(),
            format!(
                "compose\n-f\n{}\n-f\n{}\nconfig\n--format\njson\ncompose\n-f\n{}\nconfig\n--format\njson\nimage\ninspect\nexample/dev:latest\n",
                fixture.base_compose.display(),
                fixture.managed_compose.display(),
                fixture.base_compose.display()
            )
        );
    }
}

#[test]
fn rejects_devcontainer_cross_file_mismatches() {
    let _environment = ENVIRONMENT.lock().unwrap();

    let fixture = Fixture::new();
    fixture.write_devcontainer(
        "other",
        &["../compose.base.yaml", "../compose.yaml"],
        "vscode",
    );
    assert!(fixture
        .resolve()
        .unwrap_err()
        .to_string()
        .contains("service other"));

    let fixture = Fixture::new();
    fixture.write_devcontainer(
        "dev",
        &["../compose.yaml", "../compose.base.yaml"],
        "vscode",
    );
    assert!(fixture
        .resolve()
        .unwrap_err()
        .to_string()
        .contains("must be the last"));

    let fixture = Fixture::new();
    fixture.write_devcontainer("dev", &["../compose.base.yaml", "../compose.yaml"], "other");
    assert!(fixture
        .resolve()
        .unwrap_err()
        .to_string()
        .contains("remoteUser other"));

    let fixture = Fixture::new();
    fs::write(
        &fixture.base_compose,
        "name: fixture\nx-dembly:\n  schema_version: 1\nservices:\n  dev: {}\n",
    )
    .unwrap();
    assert!(fixture
        .resolve()
        .unwrap_err()
        .to_string()
        .contains("must not contain x-dembly"));
}

#[test]
fn verifies_card_filesystem_checksum_before_docker_inspection() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    fs::write(&fixture.filesystem, b"tampered").unwrap();

    let error = fixture.resolve().unwrap_err();

    assert!(error.to_string().contains("checksum mismatch"));
    assert_eq!(fixture.log(), "");
}

struct Fixture {
    root: PathBuf,
    config: PathBuf,
    base_compose: PathBuf,
    managed_compose: PathBuf,
    devcontainer: PathBuf,
    filesystem: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    compose_json: PathBuf,
    base_compose_json: PathBuf,
    image_json: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-host-plan-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let deck_root = root.join(".dembly");
        let card_root = deck_root.join("cards/tool");
        let devcontainer_root = root.join(".devcontainer");
        let bin = root.join("bin");
        fs::create_dir_all(&card_root).unwrap();
        fs::create_dir_all(&devcontainer_root).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.join("host-home")).unwrap();

        let fixture = Self {
            config: deck_root.join("config.toml"),
            base_compose: root.join("compose.base.yaml"),
            managed_compose: root.join("compose.yaml"),
            devcontainer: devcontainer_root.join("devcontainer.json"),
            filesystem: card_root.join("rootfs.squashfs"),
            log: root.join("docker.log"),
            compose_json: root.join("compose.json"),
            base_compose_json: root.join("base-compose.json"),
            image_json: root.join("image.json"),
            root,
            bin,
        };
        fixture.write_inputs();
        fixture.install_docker();
        fixture
    }

    fn write_inputs(&self) {
        fs::write(
            &self.config,
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[devcontainer]\npath = \"../.devcontainer/devcontainer.json\"\n[[cards]]\npath = \"cards/tool/card.toml\"\n[environment]\nMODE = \"deck\"\n[environment_path]\nprepend = [\"/deck/bin\"]\n",
        )
        .unwrap();
        fs::write(&self.filesystem, b"abc").unwrap();
        fs::write(
            self.filesystem.parent().unwrap().join("card.toml"),
            "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/tool\"\n[environment]\nCARD = \"enabled\"\n[environment_path]\nprepend = [\"bin\"]\n",
        )
        .unwrap();
        fs::write(
            &self.base_compose,
            "name: fixture\nservices:\n  dev:\n    image: example/dev:latest\n",
        )
        .unwrap();
        fs::write(
            &self.managed_compose,
            "name: fixture\nservices:\n  dev: {}\n",
        )
        .unwrap();
        self.write_devcontainer(
            "dev",
            &["../compose.base.yaml", "../compose.yaml"],
            "vscode",
        );
        self.write_compose_json(
            r#"{"services":{"dev":{"image":"example/dev:latest","entrypoint":["/init"],"command":["serve"],"user":"vscode:staff","environment":{"MODE":"compose","COMPOSE":"only","REMOVED":null}}}}"#,
        );
        self.write_base_compose_json(
            r#"{"services":{"dev":{"image":"example/dev:latest","environment":{}}}}"#,
        );
        self.write_image_json("image-user");
        fs::write(&self.log, "").unwrap();
    }

    fn write_devcontainer(&self, service: &str, files: &[&str], remote_user: &str) {
        let files = files
            .iter()
            .map(|file| format!("\"{file}\""))
            .collect::<Vec<_>>()
            .join(",");
        fs::write(
            &self.devcontainer,
            format!(
                "{{\"dockerComposeFile\":[{files}],\"service\":\"{service}\",\"overrideCommand\":false,\"containerUser\":\"root\",\"remoteUser\":\"{remote_user}\"}}"
            ),
        )
        .unwrap();
    }

    fn write_compose_json(&self, value: &str) {
        fs::write(&self.compose_json, value).unwrap();
    }

    fn write_base_compose_json(&self, value: &str) {
        fs::write(&self.base_compose_json, value).unwrap();
    }

    fn write_image_json(&self, user: &str) {
        fs::write(
            &self.image_json,
            format!(
                "[{{\"Id\":\"sha256:image\",\"Config\":{{\"Entrypoint\":[\"/image-init\"],\"Cmd\":[\"image-command\"],\"Env\":[\"PATH=/usr/bin\",\"MODE=image\",\"IMAGE=only\",\"REMOVED=image\"],\"User\":\"{user}\"}}}}]"
            ),
        )
        .unwrap();
    }

    fn install_docker(&self) {
        let docker = self.bin.join("docker");
        fs::write(
            &docker,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$DEMBLY_DOCKER_LOG\"\nif [ \"$1\" = compose ] && [ \"$#\" -eq 6 ]; then\n  exec /bin/cat \"$DEMBLY_BASE_COMPOSE_JSON\"\nfi\nif [ \"$1\" = compose ]; then\n  exec /bin/cat \"$DEMBLY_COMPOSE_JSON\"\nfi\nif [ \"$1\" = image ] && [ \"$2\" = inspect ]; then\n  exec /bin/cat \"$DEMBLY_IMAGE_JSON\"\nfi\nexit 91\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&docker).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(docker, permissions).unwrap();
    }

    fn resolve(&self) -> Result<HostPlan, dembly_cli::CliError> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let home = std::env::var_os("HOME");
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&path));
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        std::env::set_var("HOME", self.root.join("host-home"));
        std::env::set_var("DEMBLY_DOCKER_LOG", &self.log);
        std::env::set_var("DEMBLY_COMPOSE_JSON", &self.compose_json);
        std::env::set_var("DEMBLY_BASE_COMPOSE_JSON", &self.base_compose_json);
        std::env::set_var("DEMBLY_IMAGE_JSON", &self.image_json);
        let result = HostPlan::resolve(&self.config);
        std::env::set_var("PATH", path);
        restore("HOME", home);
        for key in [
            "DEMBLY_DOCKER_LOG",
            "DEMBLY_COMPOSE_JSON",
            "DEMBLY_IMAGE_JSON",
            "DEMBLY_BASE_COMPOSE_JSON",
        ] {
            std::env::remove_var(key);
        }
        result
    }

    fn clear_log(&self) {
        fs::write(&self.log, "").unwrap();
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn restore(key: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}
