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
fn discovery_does_not_search_parent_directories() {
    let root = temporary_directory("discovery");
    write(&root.join("deck.toml"), "schema_version = 1\nname = \"root\"\n[base]\nimage = \"alpine\"\n");
    let child = root.join("child");
    fs::create_dir_all(&child).unwrap();

    let error = discover_deck(None, &child).unwrap_err();
    assert!(error.to_string().contains("deck.toml was not found"));
}
