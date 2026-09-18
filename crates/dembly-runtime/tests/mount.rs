use dembly_runtime::{
    expand_runtime_bind_target, mount_bind, squashfs_mount_command, ResolvedRuntimeUser,
    RuntimeBind, RuntimeCard,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn card_mount_command_is_always_read_only() {
    let card = RuntimeCard {
        name: "clang".into(),
        image: PathBuf::from("/run/dembly/cards/clang.squashfs"),
        mount_target: PathBuf::from("/opt/dembly/cards/clang"),
    };
    assert_eq!(
        squashfs_mount_command(&card),
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

#[test]
fn bind_target_expands_only_runtime_user_variables() {
    let user = user();
    assert_eq!(
        expand_runtime_bind_target("${HOME}/.config/${USER}", &user).unwrap(),
        PathBuf::from("/home/vscode/.config/vscode")
    );
    for invalid in ["$HOME/.config", "${HOST_HOME}/.config", "relative/path"] {
        assert!(
            expand_runtime_bind_target(invalid, &user).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn missing_staged_bind_reports_index_source_target_and_operation() {
    let bind = RuntimeBind {
        source: PathBuf::from("/run/dembly/binds/7"),
        target: "${HOME}/.gitconfig".into(),
        mode: "ro".into(),
    };

    let error = mount_bind(&bind, &user()).unwrap_err();

    for expected in [
        "Host Bind 7",
        "/run/dembly/binds/7",
        "/home/vscode/.gitconfig",
        "inspect staged source",
    ] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn rejects_unknown_bind_mode_before_attempting_mount() {
    let root = std::env::temp_dir().join(format!("dembly-bind-mode-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let bind = RuntimeBind {
        source: root,
        target: "/tmp/dembly-target".into(),
        mode: "invalid".into(),
    };
    let error = mount_bind(&bind, &user()).unwrap_err();
    assert!(error.contains("validate mode"), "{error}");
}

#[cfg(unix)]
#[test]
fn staged_directory_and_file_binds_prepare_targets_and_apply_requested_mode() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("dembly-bind-staged-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let commands = root.join("mount.log");
    let bin = root.join("bin");
    let sources = root.join("sources");
    let home = root.join("home");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(sources.join("0")).unwrap();
    fs::write(sources.join("1"), "source").unwrap();
    fs::create_dir_all(sources.join("2")).unwrap();
    let mount = bin.join("mount");
    fs::write(
        &mount,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = '-o' ] && [ \"$3\" = '{}/failure' ]; then exit 23; fi\n",
            commands.display(),
            home.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&mount).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&mount, permissions).unwrap();

    let previous_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &bin);
    let runtime_user = ResolvedRuntimeUser {
        home: home.to_string_lossy().into_owned(),
        ..user()
    };
    let directory_result = mount_bind(
        &RuntimeBind {
            source: sources.join("0"),
            target: "${HOME}/directory".into(),
            mode: "ro".into(),
        },
        &runtime_user,
    );
    let file_result = mount_bind(
        &RuntimeBind {
            source: sources.join("1"),
            target: "${HOME}/file".into(),
            mode: "rw".into(),
        },
        &runtime_user,
    );
    let failure = mount_bind(
        &RuntimeBind {
            source: sources.join("2"),
            target: "${HOME}/failure".into(),
            mode: "ro".into(),
        },
        &runtime_user,
    )
    .unwrap_err();
    match previous_path {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }
    directory_result.unwrap();
    file_result.unwrap();

    assert!(home.join("directory").is_dir());
    assert_eq!(fs::metadata(home.join("file")).unwrap().len(), 0);
    for expected in [
        "Host Bind 2".to_owned(),
        sources.join("2").to_string_lossy().into_owned(),
        home.join("failure").to_string_lossy().into_owned(),
        "read-only remount".to_owned(),
        "23".to_owned(),
    ] {
        assert!(failure.contains(&expected), "{failure}");
    }
    assert_eq!(
        fs::read_to_string(commands).unwrap(),
        format!(
            "--bind {} {}\n-o remount,bind,ro {}\n--bind {} {}\n--bind {} {}\n-o remount,bind,ro {}\n",
            sources.join("0").display(),
            home.join("directory").display(),
            home.join("directory").display(),
            sources.join("1").display(),
            home.join("file").display(),
            sources.join("2").display(),
            home.join("failure").display(),
            home.join("failure").display()
        )
    );
}

fn user() -> ResolvedRuntimeUser {
    ResolvedRuntimeUser {
        name: "vscode".into(),
        uid: 1000,
        gid: 1000,
        home: "/home/vscode".into(),
    }
}
