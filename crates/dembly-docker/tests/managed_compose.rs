use dembly_docker::{
    atomic_create, atomic_replace, CardLock, DemblyLock, ManagedCompose, ManagedFields,
    ManagedMount, ManagedValue,
};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const COMPOSE: &str = r#"name: sample_project
x-custom:
  nested: [one, {two: 2}]
services:
  dev:
    image: alpine:3.21
    entrypoint: null
    command: ["sleep", "infinity"]
    user: "1000:1000"
    labels:
      user.example: kept
    volumes:
      - ./workspace:/workspace
  database:
    image: postgres:17
    privileged: false
    labels:
      database.example: untouched
"#;

#[test]
fn read_preserves_missing_and_explicit_null_in_service_snapshot() {
    let fixture = Fixture::new("read-values", COMPOSE);

    let compose = ManagedCompose::read(&fixture.path).unwrap();
    let service = compose.service("dev").unwrap();

    assert_eq!(compose.project_name().unwrap(), "sample_project");
    assert_eq!(
        service.entrypoint,
        ManagedValue::Present(Value::Null),
        "an explicit null is a present value"
    );
    assert_eq!(service.privileged, ManagedValue::Missing);
}

#[test]
fn set_lock_changes_only_the_embedded_lock_and_is_deterministic() {
    let source = format!("{COMPOSE}x-dembly:\n  schema_version: 1\n");
    let fixture = Fixture::new("set-lock", &source);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    let before = parse(compose.to_bytes().unwrap());
    let lock = example_lock();

    compose.set_lock(lock.clone()).unwrap();
    let first = compose.to_bytes().unwrap();
    compose.set_lock(lock.clone()).unwrap();
    let second = compose.to_bytes().unwrap();
    let after = parse(&first);

    assert_eq!(without_lock(before), without_lock(after));
    assert_eq!(compose.lock().unwrap(), Some(lock));
    assert_eq!(first, second);
}

#[test]
fn set_lock_preserves_explicit_null_state() {
    let source = format!("{COMPOSE}x-dembly:\n  schema_version: 1\n  state: null\n");
    let fixture = Fixture::new("set-lock-null-state", &source);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();

    compose.set_lock(example_lock()).unwrap();

    let rendered = parse(compose.to_bytes().unwrap());
    assert_eq!(
        x_dembly(&rendered).get(key("state")),
        Some(&Value::Null),
        "set_lock must preserve an explicit null state instead of removing its key"
    );
}

#[test]
fn set_lock_preserves_existing_state_and_service_values_semantically() {
    let fixture = Fixture::new("set-lock-state", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    compose.apply("dev", desired(), "sha256:existing").unwrap();
    let state_before = compose.state().unwrap();
    let service_before = compose.service("dev").unwrap();

    compose.set_lock(example_lock()).unwrap();

    assert_eq!(compose.state().unwrap(), state_before);
    assert_eq!(compose.service("dev").unwrap(), service_before);
}

#[test]
fn first_apply_records_exact_values_and_preserves_other_services_and_entries() {
    let fixture = Fixture::new("first-apply", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    let before_database = compose.service("database").unwrap();

    compose.apply("dev", desired(), "sha256:lock-one").unwrap();

    let state = compose.state().unwrap().unwrap();
    assert_eq!(state.service, "dev");
    assert_eq!(state.lock_digest, "sha256:lock-one");
    assert_eq!(
        state.fields.entrypoint.original,
        ManagedValue::Present(Value::Null)
    );
    assert_eq!(state.fields.privileged.original, ManagedValue::Missing);
    assert_eq!(
        state.fields.privileged.applied,
        ManagedValue::Present(Value::Bool(true))
    );
    assert_eq!(compose.service("database").unwrap(), before_database);

    let rendered = parse(compose.to_bytes().unwrap());
    let dev = service(&rendered, "dev");
    let labels = mapping_field(dev, "labels");
    assert_eq!(labels.get(key("user.example")), Some(&string("kept")));
    assert_eq!(
        labels.get(key("io.dembly.lock-digest")),
        Some(&string("sha256:lock-one"))
    );
    let volumes = sequence_field(dev, "volumes");
    assert!(volumes.contains(&string("./workspace:/workspace")));
    assert!(volumes.contains(&string(
        "/deck/.dembly/runtime/bin/dembly:/run/dembly/bin/dembly:ro"
    )));
}

#[test]
fn identical_reapply_is_byte_stable_after_initial_normalization() {
    let fixture = Fixture::new("stable-reapply", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();

    compose.apply("dev", desired(), "sha256:same").unwrap();
    let first = compose.to_bytes().unwrap();
    compose.apply("dev", desired(), "sha256:same").unwrap();
    let second = compose.to_bytes().unwrap();

    assert_eq!(first, second);
}

#[test]
fn reapply_reports_field_conflict_before_mutating_any_field() {
    let fixture = Fixture::new("reapply-conflict", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    compose.apply("dev", desired(), "sha256:old").unwrap();
    let applied = parse(compose.to_bytes().unwrap());
    compose = mutate_service_field(
        &fixture.path,
        &compose,
        "dev",
        "user",
        string("root:changed"),
    );
    let before_reapply = compose.to_bytes().unwrap();

    let conflict = compose.apply("dev", desired(), "sha256:new").unwrap_err();

    assert_eq!(conflict.service, "dev");
    assert_eq!(conflict.field, "user");
    assert_eq!(conflict.expected, ManagedValue::Present(string("root")));
    assert_eq!(
        conflict.current,
        ManagedValue::Present(string("root:changed"))
    );
    assert!(conflict.to_string().contains("service dev field user"));
    assert_eq!(compose.to_bytes().unwrap(), before_reapply);
    assert_ne!(parse(&before_reapply), applied);
}

#[test]
fn unapply_restores_missing_and_null_and_removes_only_state() {
    let fixture = Fixture::new("unapply", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    compose.set_lock(example_lock()).unwrap();
    compose.apply("dev", desired(), "sha256:lock").unwrap();
    let lock_before = compose.lock().unwrap();

    compose.unapply("dev").unwrap();

    let snapshot = compose.service("dev").unwrap();
    assert_eq!(snapshot.entrypoint, ManagedValue::Present(Value::Null));
    assert_eq!(snapshot.privileged, ManagedValue::Missing);
    assert_eq!(compose.state().unwrap(), None);
    assert_eq!(compose.lock().unwrap(), lock_before);
    let rendered = parse(compose.to_bytes().unwrap());
    let dev = service(&rendered, "dev");
    assert_eq!(
        mapping_field(dev, "labels").get(key("user.example")),
        Some(&string("kept"))
    );
    assert!(!mapping_field(dev, "labels").contains_key(key("io.dembly.lock-digest")));
    assert_eq!(
        sequence_field(dev, "volumes"),
        &vec![string("./workspace:/workspace")]
    );
}

#[test]
fn unapply_preserves_explicit_null_lock() {
    let fixture = Fixture::new("unapply-null-lock", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    compose.apply("dev", desired(), "sha256:lock").unwrap();
    let mut applied = parse(compose.to_bytes().unwrap());
    x_dembly_mut(&mut applied).insert(key("lock"), Value::Null);
    fs::write(&fixture.path, serde_yaml::to_string(&applied).unwrap()).unwrap();
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();

    compose.unapply("dev").unwrap();

    let rendered = parse(compose.to_bytes().unwrap());
    assert_eq!(
        x_dembly(&rendered).get(key("lock")),
        Some(&Value::Null),
        "unapply must preserve an explicit null lock instead of removing its key"
    );
}

#[test]
fn unapply_conflict_leaves_every_managed_value_and_state_unchanged() {
    let fixture = Fixture::new("unapply-conflict", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    compose.apply("dev", desired(), "sha256:lock").unwrap();
    compose = mutate_service_field(&fixture.path, &compose, "dev", "command", string("changed"));
    let before = compose.to_bytes().unwrap();

    let conflict = compose.unapply("dev").unwrap_err();

    assert_eq!(conflict.field, "command");
    assert_eq!(compose.to_bytes().unwrap(), before);
    assert!(compose.state().unwrap().is_some());
}

#[test]
fn sequence_labels_and_long_mounts_are_restored_without_losing_user_entries() {
    let source = r#"name: sequence_project
services:
  dev:
    labels:
      - user.example=before
      - io.dembly.lock-digest=preexisting
    volumes:
      - type: bind
        source: /user/dembly
        target: /run/dembly/bin/dembly
        read_only: false
      - /user/workspace:/workspace:rw
"#;
    let fixture = Fixture::new("sequence-labels", source);
    let original = parse(source);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();

    compose.apply("dev", desired(), "sha256:lock").unwrap();
    let applied = parse(compose.to_bytes().unwrap());
    let labels = sequence_field(service(&applied, "dev"), "labels");
    assert!(labels.contains(&string("user.example=before")));
    assert!(labels.contains(&string("io.dembly.lock-digest=sha256:lock-one")));
    assert!(
        sequence_field(service(&applied, "dev"), "volumes").contains(&string(
            "/deck/.dembly/runtime/bin/dembly:/run/dembly/bin/dembly:ro"
        ))
    );

    compose.unapply("dev").unwrap();

    assert_eq!(
        service(&parse(compose.to_bytes().unwrap()), "dev"),
        service(&original, "dev")
    );
}

#[test]
fn user_label_and_mount_changes_during_apply_are_preserved() {
    let fixture = Fixture::new("user-entry-changes", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    compose.apply("dev", desired(), "sha256:lock").unwrap();
    let mut document = parse(compose.to_bytes().unwrap());
    let dev = document
        .as_mapping_mut()
        .unwrap()
        .get_mut(key("services"))
        .unwrap()
        .as_mapping_mut()
        .unwrap()
        .get_mut(key("dev"))
        .unwrap()
        .as_mapping_mut()
        .unwrap();
    dev.get_mut(key("labels"))
        .unwrap()
        .as_mapping_mut()
        .unwrap()
        .insert(key("user.example"), string("changed"));
    dev.get_mut(key("volumes"))
        .unwrap()
        .as_sequence_mut()
        .unwrap()
        .push(string("/user/cache:/cache"));
    fs::write(&fixture.path, serde_yaml::to_string(&document).unwrap()).unwrap();
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();

    compose.unapply("dev").unwrap();

    let restored = parse(compose.to_bytes().unwrap());
    let dev = service(&restored, "dev");
    assert_eq!(
        mapping_field(dev, "labels").get(key("user.example")),
        Some(&string("changed"))
    );
    assert!(sequence_field(dev, "volumes").contains(&string("/user/cache:/cache")));
}

#[test]
fn reapply_orders_dembly_mounts_by_the_new_desired_order() {
    let fixture = Fixture::new("mount-order", COMPOSE);
    let mut compose = ManagedCompose::read(&fixture.path).unwrap();
    let mut first = desired();
    first.mounts.push(ManagedMount {
        target: "/run/dembly/cards/clang.squashfs".into(),
        value: string("/cards/clang:/run/dembly/cards/clang.squashfs:ro"),
    });
    compose.apply("dev", first, "sha256:first").unwrap();

    let mut second = desired();
    second.mounts.insert(
        0,
        ManagedMount {
            target: "/run/dembly/cards/clang.squashfs".into(),
            value: string("/cards/clang:/run/dembly/cards/clang.squashfs:ro"),
        },
    );
    compose.apply("dev", second, "sha256:second").unwrap();

    let document = parse(compose.to_bytes().unwrap());
    assert_eq!(
        sequence_field(service(&document, "dev"), "volumes"),
        &vec![
            string("./workspace:/workspace"),
            string("/cards/clang:/run/dembly/cards/clang.squashfs:ro"),
            string("/deck/.dembly/runtime/bin/dembly:/run/dembly/bin/dembly:ro"),
        ]
    );
}

#[test]
fn invalid_project_and_x_dembly_schemas_are_rejected_without_writes() {
    for (name, source, expected) in [
        (
            "missing-name",
            "services: {dev: {image: alpine}}\n",
            "top-level name",
        ),
        (
            "invalid-name-type",
            "name: [bad]\nservices: {}\n",
            "top-level name",
        ),
        (
            "invalid-name-value",
            "name: Bad.Project\nservices: {}\n",
            "invalid Compose project name",
        ),
        (
            "unknown-x-dembly",
            "name: project\nservices: {}\nx-dembly:\n  schema_version: 1\n  surprise: true\n",
            "unknown field",
        ),
        (
            "unknown-lock-field",
            "name: project\nservices: {}\nx-dembly:\n  schema_version: 1\n  lock:\n    compose_path: compose.yaml\n    service: dev\n    image: sha256:image\n    cards: []\n    surprise: true\n",
            "unknown field",
        ),
        (
            "unsupported-schema",
            "name: project\nservices: {}\nx-dembly:\n  schema_version: 2\n",
            "schema_version must be 1",
        ),
    ] {
        let fixture = Fixture::new(name, source);
        let before = fs::read(&fixture.path).unwrap();
        let error = ManagedCompose::read(&fixture.path).unwrap_err();
        assert!(error.contains(expected), "unexpected error: {error}");
        assert_eq!(fs::read(&fixture.path).unwrap(), before);
    }
}

#[test]
fn atomic_replace_preserves_permissions_and_replaces_all_bytes() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("atomic", "old");
    fs::set_permissions(&fixture.path, fs::Permissions::from_mode(0o640)).unwrap();

    atomic_replace(&fixture.path, b"new document\n").unwrap();

    assert_eq!(fs::read(&fixture.path).unwrap(), b"new document\n");
    assert_eq!(
        fs::metadata(&fixture.path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let leftovers = fs::read_dir(fixture.path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn atomic_create_publishes_complete_bytes_and_refuses_to_replace_existing_file() {
    let fixture = Fixture::new("atomic-create", "compose");
    let path = fixture.root.join("config.toml");

    atomic_create(&path, b"complete config\n").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"complete config\n");

    let error = atomic_create(&path, b"replacement\n").unwrap_err();
    assert!(error.contains("config.toml"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), b"complete config\n");
}

#[test]
fn atomic_replace_write_failure_leaves_the_target_unchanged() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("atomic-failure", "original\n");
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o500)).unwrap();

    let result = atomic_replace(&fixture.path, b"replacement\n");

    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&fixture.path).unwrap(), b"original\n");
}

fn desired() -> ManagedFields {
    ManagedFields {
        entrypoint: Value::Sequence(vec![
            string("/run/dembly/bin/dembly"),
            string("__runtime"),
            string("init"),
            string("/run/dembly/runtime/dev.toml"),
        ]),
        command: Value::Sequence(Vec::new()),
        user: string("root"),
        privileged: Value::Bool(true),
        labels: BTreeMap::from([("io.dembly.lock-digest".into(), string("sha256:lock-one"))]),
        mounts: vec![ManagedMount {
            target: "/run/dembly/bin/dembly".into(),
            value: string("/deck/.dembly/runtime/bin/dembly:/run/dembly/bin/dembly:ro"),
        }],
    }
}

fn example_lock() -> DemblyLock {
    DemblyLock {
        compose_path: "../compose.yaml".into(),
        service: "dev".into(),
        image: "sha256:image".into(),
        cards: vec![
            CardLock {
                name: "z-last".into(),
                version: "2".into(),
                manifest_sha256: "sha256:z".into(),
                filesystem_sha256: "sha256:zz".into(),
            },
            CardLock {
                name: "a-first".into(),
                version: "1".into(),
                manifest_sha256: "sha256:a".into(),
                filesystem_sha256: "sha256:aa".into(),
            },
        ],
    }
}

fn mutate_service_field(
    path: &Path,
    compose: &ManagedCompose,
    name: &str,
    field: &str,
    value: Value,
) -> ManagedCompose {
    let mut document = parse(compose.to_bytes().unwrap());
    let service = document
        .as_mapping_mut()
        .unwrap()
        .get_mut(key("services"))
        .unwrap()
        .as_mapping_mut()
        .unwrap()
        .get_mut(key(name))
        .unwrap()
        .as_mapping_mut()
        .unwrap();
    service.insert(key(field), value);
    fs::write(path, serde_yaml::to_string(&document).unwrap()).unwrap();
    ManagedCompose::read(path).unwrap()
}

fn parse(bytes: impl AsRef<[u8]>) -> Value {
    serde_yaml::from_slice(bytes.as_ref()).unwrap()
}

fn without_lock(mut document: Value) -> Value {
    if let Some(x_dembly) = document
        .as_mapping_mut()
        .and_then(|root| root.get_mut(key("x-dembly")))
        .and_then(Value::as_mapping_mut)
    {
        x_dembly.remove(key("lock"));
    }
    document
}

fn x_dembly(document: &Value) -> &Mapping {
    document
        .as_mapping()
        .unwrap()
        .get(key("x-dembly"))
        .unwrap()
        .as_mapping()
        .unwrap()
}

fn x_dembly_mut(document: &mut Value) -> &mut Mapping {
    document
        .as_mapping_mut()
        .unwrap()
        .get_mut(key("x-dembly"))
        .unwrap()
        .as_mapping_mut()
        .unwrap()
}

fn service<'a>(document: &'a Value, name: &str) -> &'a Mapping {
    document
        .as_mapping()
        .unwrap()
        .get(key("services"))
        .unwrap()
        .as_mapping()
        .unwrap()
        .get(key(name))
        .unwrap()
        .as_mapping()
        .unwrap()
}

fn mapping_field<'a>(mapping: &'a Mapping, field: &str) -> &'a Mapping {
    mapping.get(key(field)).unwrap().as_mapping().unwrap()
}

fn sequence_field<'a>(mapping: &'a Mapping, field: &str) -> &'a Vec<Value> {
    mapping.get(key(field)).unwrap().as_sequence().unwrap()
}

fn key(value: &str) -> Value {
    string(value)
}

fn string(value: &str) -> Value {
    Value::String(value.into())
}

struct Fixture {
    root: PathBuf,
    path: PathBuf,
}

impl Fixture {
    fn new(name: &str, source: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dembly-managed-compose-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("compose.yaml");
        fs::write(&path, source).unwrap();
        Self { root, path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
