use dembly_docker::{inspect_compose, inspect_image};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static ENVIRONMENT: Mutex<()> = Mutex::new(());

#[test]
fn inspect_compose_preserves_file_order_and_absent_empty_process_fields() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    fixture.write_compose_json(
        r#"{
          "services": {
            "dev": {
              "image": "registry.example/dev:latest",
              "command": [],
              "user": "1000:1001",
              "environment": {"EMPTY": "", "MODE": "development", "REMOVED": null}
            }
          }
        }"#,
    );

    let base = fixture.root.join("compose.base.yaml");
    let managed = fixture.root.join("compose.yaml");
    let service =
        fixture.with_path(|| inspect_compose(&[base.clone(), managed.clone()], "dev").unwrap());

    assert_eq!(service.image, "registry.example/dev:latest");
    assert_eq!(service.entrypoint, None);
    assert_eq!(service.command, Some(Vec::new()));
    assert_eq!(service.user.as_deref(), Some("1000:1001"));
    assert_eq!(service.environment.get("EMPTY"), Some(&Some(String::new())));
    assert_eq!(
        service.environment.get("MODE"),
        Some(&Some("development".into()))
    );
    assert_eq!(service.environment.get("REMOVED"), Some(&None));
    assert_eq!(
        fixture.log(),
        format!(
            "compose\n-f\n{}\n-f\n{}\nconfig\n--format\njson\n",
            base.display(),
            managed.display()
        )
    );

    fixture.clear_log();
    fixture.write_compose_json(
        r#"{"services":{"dev":{"image":"dev","entrypoint":[],"environment":{}}}}"#,
    );
    let service = fixture.with_path(|| inspect_compose(&[managed], "dev").unwrap());
    assert_eq!(service.entrypoint, Some(Vec::new()));
    assert_eq!(service.command, None);
    assert!(service.environment.is_empty());
}

#[test]
fn inspect_image_uses_one_read_only_inspect_call() {
    let _environment = ENVIRONMENT.lock().unwrap();
    let fixture = Fixture::new();
    fs::write(
        &fixture.image_json,
        r#"[{"Id":"sha256:abc","Config":{"Entrypoint":["/init"],"Cmd":[],"Env":["PATH=/bin","EMPTY="],"User":"vscode:staff"}}]"#,
    )
    .unwrap();

    let image = fixture.with_path(|| inspect_image("registry.example/dev:latest").unwrap());

    assert_eq!(image.id, "sha256:abc");
    assert_eq!(image.entrypoint, vec!["/init"]);
    assert!(image.command.is_empty());
    assert_eq!(image.environment, vec!["PATH=/bin", "EMPTY="]);
    assert_eq!(image.user, "vscode:staff");
    assert_eq!(
        fixture.log(),
        "image\ninspect\nregistry.example/dev:latest\n"
    );
}

struct Fixture {
    root: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    compose_json: PathBuf,
    image_json: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "dembly-compose-config-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let fixture = Self {
            log: root.join("docker.log"),
            compose_json: root.join("compose.json"),
            image_json: root.join("image.json"),
            root,
            bin,
        };
        fixture.install_docker();
        fixture
    }

    fn install_docker(&self) {
        let docker = self.bin.join("docker");
        fs::write(
            &docker,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$DEMBLY_DOCKER_LOG\"\nif [ \"$1\" = compose ]; then\n  exec /bin/cat \"$DEMBLY_COMPOSE_JSON\"\nfi\nif [ \"$1\" = image ] && [ \"$2\" = inspect ]; then\n  exec /bin/cat \"$DEMBLY_IMAGE_JSON\"\nfi\nexit 91\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&docker).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(docker, permissions).unwrap();
    }

    fn write_compose_json(&self, value: &str) {
        fs::write(&self.compose_json, value).unwrap();
    }

    fn clear_log(&self) {
        fs::write(&self.log, "").unwrap();
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    fn with_path<T>(&self, operation: impl FnOnce() -> T) -> T {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![self.bin.clone()];
        paths.extend(std::env::split_paths(&path));
        let joined = std::env::join_paths(paths).unwrap();
        let previous_log = std::env::var_os("DEMBLY_DOCKER_LOG");
        let previous_compose = std::env::var_os("DEMBLY_COMPOSE_JSON");
        let previous_image = std::env::var_os("DEMBLY_IMAGE_JSON");
        std::env::set_var("PATH", joined);
        std::env::set_var("DEMBLY_DOCKER_LOG", &self.log);
        std::env::set_var("DEMBLY_COMPOSE_JSON", &self.compose_json);
        std::env::set_var("DEMBLY_IMAGE_JSON", &self.image_json);
        let result = operation();
        std::env::set_var("PATH", path);
        restore("DEMBLY_DOCKER_LOG", previous_log);
        restore("DEMBLY_COMPOSE_JSON", previous_compose);
        restore("DEMBLY_IMAGE_JSON", previous_image);
        result
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
