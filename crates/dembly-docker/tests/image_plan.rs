use dembly_docker::{image_create_command, CardFileBind, ImageRuntimePlan};
use std::path::PathBuf;

#[test]
fn image_runtime_plan_binds_same_binary_and_card_files_read_only() {
    let plan = ImageRuntimePlan {
        image: "alpine:3.20".into(),
        container_name: "dembly-example".into(),
        executable: PathBuf::from("/host/dembly"),
        runtime_config: PathBuf::from("/tmp/runtime.toml"),
        cards: vec![CardFileBind {
            name: "clang".into(),
            source: PathBuf::from("/cards/clang/rootfs.squashfs"),
        }],
        labels: vec![
            ("io.dembly.managed".into(), "true".into()),
            ("io.dembly.deck".into(), "example".into()),
        ],
        extra_mounts: Vec::new(),
    };

    let command = image_create_command(&plan);
    let arguments = command
        .iter()
        .map(|value| value.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(arguments.contains("docker create --name dembly-example --privileged"));
    assert!(arguments.contains("/host/dembly:/run/dembly/bin/dembly:ro"));
    assert!(arguments.contains("/cards/clang/rootfs.squashfs:/run/dembly/cards/clang.squashfs:ro"));
    assert!(arguments.contains("io.dembly.managed=true"));
    assert!(arguments.contains("--user 0:0"));
    assert!(arguments.contains("--entrypoint /run/dembly/bin/dembly"));
    assert!(arguments.ends_with("alpine:3.20 __runtime init /run/dembly/runtime.toml"));
}
