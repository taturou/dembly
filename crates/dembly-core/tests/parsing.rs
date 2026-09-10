use dembly_core::{discover_deck, load_card, load_deck};
use std::fs;
use std::path::Path;

fn temporary_directory(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("dembly-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

#[test]
fn deck_parser_rejects_unknown_fields() {
    let directory = temporary_directory("unknown-deck");
    let deck = directory.join("deck.toml");
    write(
        &deck,
        "schema_version = 1\nname = \"example\"\nunknown = true\n[base]\nimage = \"alpine:3.20\"\n",
    );

    let error = load_deck(&deck).unwrap_err();
    assert!(error.to_string().contains("unknown field: unknown"));
}

#[test]
fn card_parser_reads_required_squashfs_fields() {
    let directory = temporary_directory("valid-card");
    let card = directory.join("card.toml");
    write(
        &card,
        "schema_version = 1\nname = \"clang\"\nversion = \"20.1.0\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"abc\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n",
    );

    let card = load_card(&card).unwrap();
    assert_eq!(card.name, "clang");
    assert_eq!(card.filesystem.file, "rootfs.squashfs");
}

#[test]
fn card_parser_accepts_generated_environment_path_entries() {
    let directory = temporary_directory("card-environment-path");
    let card = directory.join("card.toml");
    write(
        &card,
        "schema_version = 1\nname = \"clang\"\nversion = \"20.1.0\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"abc\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n[environment_path]\nprepend = [\"bin\", \"tools/bin\"]\n",
    );

    let card = load_card(&card).unwrap();
    assert_eq!(card.environment_path_prepend, ["bin", "tools/bin"]);
}

#[test]
fn manifests_parse_volume_bind_export_hook_and_check_declarations() {
    let directory = temporary_directory("full-manifests");
    let card_path = directory.join("card.toml");
    write(
        &card_path,
        "schema_version = 1\nname = \"clang\"\nversion = \"20.1.0\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"abc\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n[environment]\nCLANG_RESOURCE = \"example\"\n[[exports]]\nsource = \"bin/clang\"\ntarget = \"/usr/local/bin/clang\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/var/cache/clang\"\n[[binds]]\nsource = \"${HOST_HOME}/.config\"\ntarget = \"${HOME}/.config\"\nmode = \"ro\"\nrequired = false\n[[hooks.post_mount]]\nexec = \"setup/post-mount.sh\"\nargs = [\"--example\"]\n[check]\nexec = \"bin/clang\"\nargs = [\"--version\"]\n",
    );
    let card = load_card(&card_path).unwrap();
    assert_eq!(card.environment["CLANG_RESOURCE"], "example");
    assert_eq!(card.exports[0].target, "/usr/local/bin/clang");
    assert!(!card.binds[0].required);
    assert_eq!(card.post_mount_hooks[0].args, ["--example"]);
    assert_eq!(card.check.unwrap().exec, "bin/clang");

    let deck_path = directory.join("deck.toml");
    write(
        &deck_path,
        "schema_version = 1\nname = \"example\"\n[base]\nimage = \"alpine:3.20\"\n[environment]\nMODE = \"development\"\n[environment_path]\nprepend = [\"/project/bin\"]\n[[volumes]]\nname = \"build\"\ntarget = \"/workspace/build\"\n[[binds]]\nsource = \".\"\ntarget = \"/workspace\"\nmode = \"rw\"\n",
    );
    let deck = load_deck(&deck_path).unwrap();
    assert_eq!(deck.environment["MODE"], "development");
    assert_eq!(deck.volumes[0].name, "build");
    assert!(deck.binds[0].required);
}

#[test]
fn discovery_does_not_search_parent_directories() {
    let root = temporary_directory("discovery");
    write(
        &root.join("deck.toml"),
        "schema_version = 1\nname = \"root\"\n[base]\nimage = \"alpine\"\n",
    );
    let child = root.join("child");
    fs::create_dir_all(&child).unwrap();

    let error = discover_deck(None, &child).unwrap_err();
    assert!(error.to_string().contains("deck.toml was not found"));
}
