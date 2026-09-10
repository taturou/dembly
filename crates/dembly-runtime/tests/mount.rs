use dembly_runtime::{squashfs_mount_command, RuntimeCard};
use std::path::PathBuf;

#[test]
fn runtime_mount_uses_external_read_only_kernel_squashfs_command() {
    let card = RuntimeCard {
        name: "clang".into(),
        image: PathBuf::from("/run/dembly/cards/clang.squashfs"),
        mount_target: PathBuf::from("/opt/dembly/cards/clang"),
    };
    let command = squashfs_mount_command(&card);
    assert_eq!(
        command,
        vec![
            "mount",
            "-t",
            "squashfs",
            "-o",
            "loop,ro",
            "/run/dembly/cards/clang.squashfs",
            "/opt/dembly/cards/clang",
        ]
    );
}
