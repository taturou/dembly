use dembly_docker::{compose_override, ComposeRuntimePlan};
use std::path::PathBuf;

#[test]
fn compose_override_changes_only_selected_service_with_internal_entrypoint() {
    let plan = ComposeRuntimePlan {
        service: "dev".into(),
        executable: PathBuf::from("/host/dembly"),
        runtime_config: PathBuf::from("/tmp/runtime.toml"),
        mounts: vec![(PathBuf::from("/cards/clang/rootfs.squashfs"), "/run/dembly/cards/clang.squashfs".into(), true)],
        labels: vec![("io.dembly.managed".into(), "true".into())],
    };
    let override_file = compose_override(&plan);
    assert!(override_file.contains("services:\n  dev:"));
    assert!(override_file.contains("privileged: true"));
    assert!(override_file.contains("entrypoint: [\"/run/dembly/bin/dembly\", \"__runtime\", \"init\", \"/run/dembly/runtime.toml\"]"));
    assert!(override_file.contains("/host/dembly:/run/dembly/bin/dembly:ro"));
    assert!(!override_file.contains("database:"));
}
