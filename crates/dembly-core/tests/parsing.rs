use dembly_core::{discover_config, load_card, load_config, resolve_deck, HostVariables};
use std::fs;
use std::path::{Path, PathBuf};

fn temporary_directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dembly-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn config(compose: &str) -> String {
    format!("schema_version = 1\n[compose]\npath = \"{compose}\"\nservice = \"dev\"\n")
}

#[test]
fn config_discovery_uses_only_the_current_directory_or_explicit_path() {
    let root = temporary_directory("config-discovery");
    let cwd = root.join("project");
    write(&cwd.join(".dembly/config.toml"), &config("compose.yaml"));
    write(&cwd.join("custom/config.toml"), &config("compose.yaml"));

    assert_eq!(
        discover_config(None, &cwd).unwrap(),
        cwd.join(".dembly/config.toml")
    );
    assert_eq!(
        discover_config(Some(Path::new("custom/config.toml")), &cwd).unwrap(),
        cwd.join("custom/config.toml")
    );

    let child = cwd.join("child");
    fs::create_dir_all(&child).unwrap();
    let error = discover_config(None, &child).unwrap_err();
    assert!(error.to_string().contains("config.toml"));

    let error = discover_config(Some(Path::new("missing.toml")), &cwd).unwrap_err();
    assert!(error.to_string().contains("missing.toml"));
}

#[test]
fn config_parser_requires_compose_and_rejects_unknown_fields_and_schema_versions() {
    let directory = temporary_directory("invalid-config");
    let missing_compose = directory.join("missing-compose.toml");
    write(&missing_compose, "schema_version = 1\n");
    assert!(load_config(&missing_compose).is_err());

    let unknown = directory.join("unknown.toml");
    write(&unknown, "schema_version = 1\nunknown = true\n[compose]\npath = \"compose.yaml\"\nservice = \"dev\"\n");
    assert!(load_config(&unknown)
        .unwrap_err()
        .to_string()
        .contains("unknown field `unknown`"));

    let unsupported_schema = directory.join("unsupported-schema.toml");
    write(
        &unsupported_schema,
        "schema_version = 2\n[compose]\npath = \"compose.yaml\"\nservice = \"dev\"\n",
    );
    assert!(load_config(&unsupported_schema)
        .unwrap_err()
        .to_string()
        .contains("Config schema_version must be 1"));
}

#[test]
fn config_parser_reads_optional_devcontainer_and_resolves_card_paths_from_deck_root() {
    let project = temporary_directory("config-card-paths");
    let config_path = project.join(".dembly/config.toml");
    let absolute_card = project.join("shared/absolute-card.toml");
    let relative_card = project.join(".dembly/cards/local/card.toml");
    let card = "schema_version = 1\nname = \"clang\"\nversion = \"20.1.0\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n";
    write(&absolute_card, card);
    write(
        &relative_card,
        card.replace("name = \"clang\"", "name = \"local\"")
            .replace(
                "target = \"/opt/dembly/cards/clang\"",
                "target = \"/opt/dembly/cards/local\"",
            )
            .as_str(),
    );
    write(
        &config_path,
        format!("schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[devcontainer]\npath = \"../devcontainer.json\"\n[[cards]]\npath = \"{}\"\n[[cards]]\npath = \"cards/local/card.toml\"\n", absolute_card.display()).as_str(),
    );

    let document = load_config(&config_path).unwrap();
    assert_eq!(document.compose.service, "dev");
    assert_eq!(document.devcontainer.unwrap().path, "../devcontainer.json");

    let resolved = resolve_deck(
        &config_path,
        &HostVariables {
            host_home: PathBuf::from("/home/host"),
            deck_root: config_path.parent().unwrap().to_path_buf(),
        },
    )
    .unwrap();
    assert_eq!(resolved.cards[0].manifest_path, absolute_card);
    assert_eq!(resolved.cards[1].manifest_path, relative_card);
    assert_eq!(resolved.root, config_path.parent().unwrap());
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
fn card_parser_preserves_generated_declarations() {
    let directory = temporary_directory("full-card");
    let card_path = directory.join("card.toml");
    write(
        &card_path,
        "schema_version = 1\nname = \"clang\"\nversion = \"20.1.0\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"abc\"\n[mount]\ntarget = \"/opt/dembly/cards/clang\"\n[environment]\nCLANG_RESOURCE = \"example\"\n[environment_path]\nprepend = [\"bin\", \"tools/bin\"]\n[[exports]]\nsource = \"bin/clang\"\ntarget = \"/usr/local/bin/clang\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/var/cache/clang\"\n[[binds]]\nsource = \"${HOST_HOME}/.config\"\ntarget = \"${HOME}/.config\"\nmode = \"ro\"\nrequired = false\n[[hooks.post_mount]]\nexec = \"setup/post-mount.sh\"\nargs = [\"--example\"]\n[check]\nexec = \"bin/clang\"\nargs = [\"--version\"]\n",
    );

    let card = load_card(&card_path).unwrap();
    assert_eq!(card.environment["CLANG_RESOURCE"], "example");
    assert_eq!(card.environment_path_prepend, ["bin", "tools/bin"]);
    assert_eq!(card.exports[0].target, "/usr/local/bin/clang");
    assert!(!card.binds[0].required);
    assert_eq!(card.post_mount_hooks[0].args, ["--example"]);
    assert_eq!(card.check.unwrap().exec, "bin/clang");
}
