use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn image_base_applies_test_card_manifest_without_host_squashfs_mount() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = repository.join("tests/fixtures/hello-card/rootfs");
    let image_dockerfile = repository.join("tests/fixtures/image-base");
    let tag = format!("dembly-fixture-{}", std::process::id());
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    run(Command::new("docker").args(["build", "-t", &tag, image_dockerfile.to_str().unwrap()]));
    run(Command::new(&binary).args([
        "card",
        "build",
        fixture.to_str().unwrap(),
        root.join("cards").to_str().unwrap(),
        "--name",
        "hello",
        "--version",
        "1",
        "--mount-target",
        "/opt/dembly/cards/hello",
        "--path-prepend",
        "bin",
        "--non-interactive",
    ]));
    build_test_card(
        &binary,
        &fixture,
        &root.join("cards"),
        "second",
        "/opt/dembly/cards/second",
    );
    let manifest_path = root.join("cards/hello/card.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        format!(
            "{manifest}\n[environment]\nHELLO_ENV = \"enabled\"\n\n[[exports]]\nsource = \"bin/hello\"\ntarget = \"/usr/local/bin/hello\"\n\n[[hooks.post_mount]]\nexec = \"setup/post-mount.sh\"\n\n[check]\nexec = \"bin/hello\"\n"
        ),
    )
    .unwrap();
    fs::write(root.join("host-input"), b"host-input\n").unwrap();
    fs::create_dir_all(root.join("host-rw")).unwrap();
    fs::write(root.join("deck.toml"), format!("schema_version = 1\nname = \"fixture-{}\"\n[base]\nimage = \"{tag}\"\n[[cards]]\npath = \"cards/hello/card.toml\"\n[[cards]]\npath = \"cards/second/card.toml\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/work/cache\"\n[[binds]]\nsource = \"${{DECK_ROOT}}/host-input\"\ntarget = \"/work/input\"\nmode = \"ro\"\n[[binds]]\nsource = \"${{DECK_ROOT}}/host-rw\"\ntarget = \"/work/rw\"\nmode = \"rw\"\n", std::process::id())).unwrap();

    let acceptance = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(
        acceptance.status.success(),
        "{}",
        String::from_utf8_lossy(&acceptance.stderr)
    );
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    run(Command::new(&binary).current_dir(&root).args([
        "run",
        "--",
        "/bin/sh",
        "-c",
        "test -x /opt/dembly/cards/hello/bin/hello && test -x /opt/dembly/cards/second/bin/hello",
    ]));
    run(Command::new(&binary).current_dir(&root).arg("check"));
    run(Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/usr/local/bin/hello"]));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/usr/local/bin/hello"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello-card");
    let second_output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/usr/local/bin/hello"])
        .output()
        .unwrap();
    assert!(
        second_output.status.success(),
        "{}",
        String::from_utf8_lossy(&second_output.stderr)
    );
    let write_bind = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/sh", "-c", "printf x > /work/input"])
        .output()
        .unwrap();
    assert!(!write_bind.status.success());
    assert_eq!(
        fs::read_to_string(root.join("host-input")).unwrap(),
        "host-input\n"
    );
    let write_rw_bind = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/sh", "-c", "printf rw > /work/rw/result"])
        .output()
        .unwrap();
    assert!(
        write_rw_bind.status.success(),
        "{}",
        String::from_utf8_lossy(&write_rw_bind.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.join("host-rw/result")).unwrap(),
        "rw"
    );
    let check_output = Command::new(&binary)
        .current_dir(&root)
        .arg("check")
        .output()
        .unwrap();
    assert!(
        check_output.status.success(),
        "{}",
        String::from_utf8_lossy(&check_output.stderr)
    );
    assert!(String::from_utf8_lossy(&check_output.stdout).contains("check hello"));
    run(Command::new(&binary).current_dir(&root).arg("up"));
    let exec_output = Command::new(&binary)
        .current_dir(&root)
        .args(["exec", "--", "/usr/local/bin/hello"])
        .output()
        .unwrap();
    assert!(
        exec_output.status.success(),
        "{}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&exec_output.stdout).trim(),
        "hello-card"
    );
    run(Command::new(&binary).current_dir(&root).arg("down"));
    let container_name = format!("dembly-fixture-{}", std::process::id());
    let containers = Command::new("docker")
        .args(["container", "ls", "--all", "--filter"])
        .arg(format!("name=^/{container_name}$"))
        .args(["--format", "{{.ID}}"])
        .output()
        .unwrap();
    assert!(containers.status.success());
    assert!(containers.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("volumes/cache/result")).unwrap(),
        "persisted\npersisted\npersisted\npersisted\npersisted\npersisted\n"
    );
    let image_id = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", &tag])
        .output()
        .unwrap();
    assert!(image_id.status.success());
    let deck_path = root.join("deck.toml");
    let deck = fs::read_to_string(&deck_path).unwrap();
    fs::write(
        &deck_path,
        deck.replace("[[cards]]\npath = \"cards/second/card.toml\"\n", ""),
    )
    .unwrap();
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    run(Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/true"]));
    let changed_image_id = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", &tag])
        .output()
        .unwrap();
    assert!(changed_image_id.status.success());
    assert_eq!(image_id.stdout, changed_image_id.stdout);
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("version = \"1\"", "version = \"2\""),
    )
    .unwrap();
    let stale_lock = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/true"])
        .output()
        .unwrap();
    assert!(!stale_lock.status.success());
    assert!(String::from_utf8_lossy(&stale_lock.stderr).contains("lock"));
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("exec = \"bin/hello\"", "exec = \"bin/missing\""),
    )
    .unwrap();
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let failed_check = Command::new(&binary)
        .current_dir(&root)
        .arg("check")
        .output()
        .unwrap();
    assert!(!failed_check.status.success());
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("setup/post-mount.sh", "setup/missing-post-mount.sh"),
    )
    .unwrap();
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let failed_hook = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/true"])
        .output()
        .unwrap();
    assert!(!failed_hook.status.success());
    let filesystem_path = root.join("cards/hello/rootfs.squashfs");
    let mut filesystem = fs::read(&filesystem_path).unwrap();
    filesystem.push(0);
    fs::write(&filesystem_path, filesystem).unwrap();
    let tampered_run = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/true"])
        .output()
        .unwrap();
    assert!(!tampered_run.status.success());
    assert!(String::from_utf8_lossy(&tampered_run.stderr).contains("checksum"));
    let host_mounts = fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(!host_mounts.contains("/opt/dembly/cards/hello"));
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", &tag])
        .status();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_rejects_duplicate_card_names_from_distinct_manifests() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = repository.join("tests/fixtures/hello-card/rootfs");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    for (directory, target) in [
        ("one", "/opt/dembly/cards/one"),
        ("two", "/opt/dembly/cards/two"),
    ] {
        build_test_card(
            &binary,
            &fixture,
            &root.join(directory),
            "duplicate",
            target,
        );
    }
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"duplicate-card\"\n[base]\nimage = \"alpine:3.21\"\n[[cards]]\npath = \"one/duplicate/card.toml\"\n[[cards]]\npath = \"two/duplicate/card.toml\"\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate Card name"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_rejects_duplicate_export_targets() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = repository.join("tests/fixtures/hello-card/rootfs");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    for (directory, name, target) in [
        ("one", "one", "/opt/dembly/cards/one"),
        ("two", "two", "/opt/dembly/cards/two"),
    ] {
        build_test_card(&binary, &fixture, &root.join(directory), name, target);
        let manifest = root.join(directory).join(name).join("card.toml");
        let content = fs::read_to_string(&manifest).unwrap();
        fs::write(
            manifest,
            format!("{content}\n[[exports]]\nsource = \"bin/hello\"\ntarget = \"/usr/local/bin/test-tool\"\n"),
        )
        .unwrap();
    }
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"duplicate-export\"\n[base]\nimage = \"alpine:3.21\"\n[[cards]]\npath = \"one/one/card.toml\"\n[[cards]]\npath = \"two/two/card.toml\"\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate export target"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_rejects_exact_card_mount_target_collisions() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = repository.join("tests/fixtures/hello-card/rootfs");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    build_test_card(
        &binary,
        &fixture,
        &root.join("one"),
        "one",
        "/opt/dembly/cards/shared",
    );
    build_test_card(
        &binary,
        &fixture,
        &root.join("two"),
        "two",
        "/opt/dembly/cards/shared",
    );
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"duplicate-mount\"\n[base]\nimage = \"alpine:3.21\"\n[[cards]]\npath = \"one/one/card.toml\"\n[[cards]]\npath = \"two/two/card.toml\"\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mount target collision"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_warns_and_skips_an_optional_missing_bind() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"optional-bind\"\n[base]\nimage = \"alpine:3.21\"\n[[binds]]\nsource = \"missing\"\ntarget = \"/work/missing\"\nmode = \"ro\"\nrequired = false\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("optional Host Bind source does not exist; skipping"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_rejects_a_required_missing_bind() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"required-bind\"\n[base]\nimage = \"alpine:3.21\"\n[[binds]]\nsource = \"missing\"\ntarget = \"/work/missing\"\nmode = \"ro\"\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing required Host Bind source"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inspect_expands_host_and_runtime_bind_variables() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let host_home = std::env::var("HOME").unwrap();

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"bind-variables\"\n[base]\nimage = \"alpine:3.21\"\n[[binds]]\nsource = \"${HOST_HOME}\"\ntarget = \"/work/${USER}\"\nmode = \"ro\"\n[[binds]]\nsource = \"${HOST_HOME}\"\ntarget = \"${HOME}/config\"\nmode = \"ro\"\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("inspect")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("canonical={host_home}")));
    assert!(stdout.contains("target=/work/root"));
    assert!(stdout.contains("target=/root/config"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inspect_uses_private_card_and_shared_volume_layouts() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = repository.join("tests/fixtures/hello-card/rootfs");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");

    build_test_card(
        &binary,
        &fixture,
        &root.join("cards"),
        "private",
        "/opt/private",
    );
    build_test_card(
        &binary,
        &fixture,
        &root.join("cards"),
        "shared",
        "/opt/shared",
    );
    for (name, declaration) in [
        (
            "private",
            "name = \"private_cache\"\ntarget = \"/cache/private\"",
        ),
        (
            "shared",
            "name = \"shared_cache\"\ntarget = \"/cache/shared\"\nshared = true",
        ),
    ] {
        let manifest = root.join("cards").join(name).join("card.toml");
        let content = fs::read_to_string(&manifest).unwrap();
        fs::write(manifest, format!("{content}\n[[volumes]]\n{declaration}\n")).unwrap();
    }
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"volume-layout\"\n[base]\nimage = \"alpine:3.21\"\n[[cards]]\npath = \"cards/private/card.toml\"\n[[cards]]\npath = \"cards/shared/card.toml\"\n",
    )
    .unwrap();

    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("inspect")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "source={}/volumes/private/private_cache",
        root.display()
    )));
    assert!(stdout.contains(&format!("source={}/volumes/shared_cache", root.display())));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_rejects_mixed_shared_volume_declarations() {
    let root = temporary_directory();
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"mixed-volume\"\n[base]\nimage = \"alpine:3.21\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/one\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/two\"\nshared = true\n",
    )
    .unwrap();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mixes shared=true and shared=false"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_rejects_a_symlinked_volume_path() {
    let root = temporary_directory();
    fs::create_dir_all(root.join("volumes")).unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), root.join("volumes/cache")).unwrap();
    fs::write(
        root.join("deck.toml"),
        "schema_version = 1\nname = \"symlink-volume\"\n[base]\nimage = \"alpine:3.21\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/cache\"\n",
    )
    .unwrap();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let output = Command::new(&binary)
        .current_dir(&root)
        .arg("validate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Volume physical path is a symlink"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn image_base_executes_a_symlink_from_a_card_filesystem() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let image_dockerfile = repository.join("tests/fixtures/image-base");
    let tag = format!("dembly-symlink-fixture-{}", std::process::id());
    let tool_root = root.join("tool-root");

    fs::create_dir_all(tool_root.join("bin")).unwrap();
    let target = tool_root.join("bin/true-target");
    fs::write(&target, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("true-target", tool_root.join("bin/true-link")).unwrap();
    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    run(Command::new("docker").args(["build", "-t", &tag, image_dockerfile.to_str().unwrap()]));
    build_test_card(
        &binary,
        &tool_root,
        &root.join("cards"),
        "symlink",
        "/opt/dembly/cards/symlink",
    );
    fs::write(
        root.join("deck.toml"),
        format!("schema_version = 1\nname = \"symlink-fixture-{}\"\n[base]\nimage = \"{tag}\"\n[[cards]]\npath = \"cards/symlink/card.toml\"\n", std::process::id()),
    )
    .unwrap();

    run(Command::new(&binary).current_dir(&root).arg("lock"));
    run(Command::new(&binary).current_dir(&root).args([
        "run",
        "--",
        "/opt/dembly/cards/symlink/bin/true-link",
    ]));
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", &tag])
        .status();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn image_base_executes_a_mmap_backed_executable_from_a_card_filesystem() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let image_dockerfile = repository.join("tests/fixtures/image-base");
    let tag = format!("dembly-mmap-fixture-{}", std::process::id());
    let tool_root = root.join("tool-root");
    let source = root.join("mmap.rs");

    fs::create_dir_all(tool_root.join("bin")).unwrap();
    fs::write(
        &source,
        "use std::ffi::c_void;\nunsafe extern \"C\" { fn mmap(address: *mut c_void, length: usize, protection: i32, flags: i32, file: i32, offset: isize) -> *mut c_void; fn munmap(address: *mut c_void, length: usize) -> i32; }\nfn main() { unsafe { let page = mmap(std::ptr::null_mut(), 4096, 1 | 2, 2 | 0x20, -1, 0); if page as isize == -1 { std::process::exit(2); } *(page as *mut u8) = 42; if *(page as *const u8) != 42 || munmap(page, 4096) != 0 { std::process::exit(3); } } println!(\"mmap-card\"); }\n",
    )
    .unwrap();
    run(Command::new("rustc").args([
        "--target",
        "x86_64-unknown-linux-musl",
        "-O",
        source.to_str().unwrap(),
        "-o",
        tool_root.join("bin/mmap-card").to_str().unwrap(),
    ]));
    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    run(Command::new("docker").args(["build", "-t", &tag, image_dockerfile.to_str().unwrap()]));
    build_test_card(
        &binary,
        &tool_root,
        &root.join("cards"),
        "mmap",
        "/opt/dembly/cards/mmap",
    );
    fs::write(
        root.join("deck.toml"),
        format!("schema_version = 1\nname = \"mmap-fixture-{}\"\n[base]\nimage = \"{tag}\"\n[[cards]]\npath = \"cards/mmap/card.toml\"\n", std::process::id()),
    )
    .unwrap();

    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/opt/dembly/cards/mmap/bin/mmap-card"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "mmap-card");
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", &tag])
        .status();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn image_base_preserves_non_root_runtime_user() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let image_dockerfile = repository.join("tests/fixtures/nonroot-base");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let tag = format!("dembly-nonroot-fixture-{}", std::process::id());
    let deck_name = format!("nonroot-fixture-{}", std::process::id());

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    run(Command::new("docker").args(["build", "-t", &tag, image_dockerfile.to_str().unwrap()]));
    fs::write(
        root.join("deck.toml"),
        format!("schema_version = 1\nname = \"{deck_name}\"\n[base]\nimage = \"{tag}\"\n"),
    )
    .unwrap();

    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let run_output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "id", "-u"])
        .output()
        .unwrap();
    assert!(
        run_output.status.success(),
        "{}",
        String::from_utf8_lossy(&run_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run_output.stdout).trim(), "65534");
    run(Command::new(&binary).current_dir(&root).arg("up"));
    let exec_output = Command::new(&binary)
        .current_dir(&root)
        .args(["exec", "--", "id", "-u"])
        .output()
        .unwrap();
    assert!(
        exec_output.status.success(),
        "{}",
        String::from_utf8_lossy(&exec_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&exec_output.stdout).trim(), "65534");
    run(Command::new(&binary).current_dir(&root).arg("down"));
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", &tag])
        .status();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_base_runs_a_temporary_command_and_cleans_up() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let deck_name = format!("compose-fixture-{}", std::process::id());

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::write(
        root.join("compose.yaml"),
        "services:\n  dev:\n    image: alpine:3.21\n    command: [\"sleep\", \"infinity\"]\n    stop_grace_period: 1s\n  sidecar:\n    image: alpine:3.21\n    command: [\"sleep\", \"infinity\"]\n    stop_grace_period: 1s\n",
    )
    .unwrap();
    fs::write(
        root.join("deck.toml"),
        format!(
            "schema_version = 1\nname = \"{deck_name}\"\n[base]\ncompose = \"compose.yaml\"\nservice = \"dev\"\n[environment]\nCOMPOSE_ENV = \"enabled\"\n"
        ),
    )
    .unwrap();

    run(Command::new(&binary).current_dir(&root).arg("lock"));
    let output = Command::new(&binary)
        .current_dir(&root)
        .args(["run", "--", "/bin/echo", "compose-runtime"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("compose-runtime"));
    run(Command::new(&binary).current_dir(&root).arg("up"));
    let project = format!("dembly-{deck_name}");
    let sidecar = Command::new("docker")
        .args(["compose", "-p", &project, "-f"])
        .arg(root.join("compose.yaml"))
        .args(["exec", "-T", "sidecar", "/bin/echo", "sidecar-alive"])
        .output()
        .unwrap();
    assert!(
        sidecar.status.success(),
        "{}",
        String::from_utf8_lossy(&sidecar.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&sidecar.stdout).trim(),
        "sidecar-alive"
    );
    let output = Command::new(&binary)
        .current_dir(&root)
        .args([
            "exec",
            "--",
            "/bin/sh",
            "-c",
            "test \"$COMPOSE_ENV\" = enabled && echo compose-exec",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "compose-exec"
    );
    run(Command::new(&binary).current_dir(&root).arg("down"));
    fs::write(
        root.join("compose.yaml"),
        "services:\n  dev:\n    image: alpine:3.21\n    command: [\"/bin/true\"]\n",
    )
    .unwrap();
    run(Command::new(&binary).current_dir(&root).arg("lock"));
    run(Command::new(&binary).current_dir(&root).arg("check"));
    let containers = Command::new("docker")
        .args(["compose", "-p", &project, "-f"])
        .arg(root.join("compose.yaml"))
        .args(["ps", "--all", "-q"])
        .output()
        .unwrap();
    assert!(containers.status.success());
    assert!(containers.stdout.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn card_build_interactive_creates_an_artifact_from_prompted_values() {
    let root = temporary_directory();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = repository.join("target/x86_64-unknown-linux-musl/release/dembly");
    let tool_root = root.join("interactive-tool");

    run(Command::new("cargo").current_dir(&repository).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "dembly-cli",
    ]));
    fs::create_dir_all(tool_root.join("bin")).unwrap();
    fs::write(tool_root.join("bin/tool"), b"#!/bin/sh\nexit 0\n").unwrap();
    let mut child = Command::new(&binary)
        .args(["card", "build"])
        .arg(&tool_root)
        .arg(root.join("cards"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\n1.0.0\n\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(root.join("cards/interactive-tool/card.toml")).unwrap();
    assert!(manifest.contains("name = \"interactive-tool\""));
    assert!(manifest.contains("version = \"1.0.0\""));
    assert!(root
        .join("cards/interactive-tool/rootfs.squashfs")
        .is_file());
    let _ = fs::remove_dir_all(root);
}

fn temporary_directory() -> PathBuf {
    let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "dembly-integration-{}-{sequence}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn build_test_card(
    binary: &std::path::Path,
    fixture: &std::path::Path,
    cards_root: &std::path::Path,
    name: &str,
    mount_target: &str,
) {
    run(Command::new(binary).args([
        "card",
        "build",
        fixture.to_str().unwrap(),
        cards_root.to_str().unwrap(),
        "--name",
        name,
        "--version",
        "1",
        "--mount-target",
        mount_target,
        "--non-interactive",
    ]));
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
