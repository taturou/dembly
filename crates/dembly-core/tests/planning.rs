use dembly_core::{
    expand_bind_source, expand_bind_target, plan_environment, resolve_deck, resolve_volume_path,
    validate_mount_targets, validate_shared_volume_consistency, CardEnvironment, HostVariables,
    MountResource, VolumeOwner,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn variables() -> HostVariables {
    HostVariables {
        host_home: PathBuf::from("/home/host"),
        deck_root: PathBuf::from("/work/deck"),
    }
}

#[test]
fn bind_source_expands_only_host_variables_and_target_keeps_runtime_variables() {
    assert_eq!(
        expand_bind_source("${HOST_HOME}/.config", &variables()).unwrap(),
        PathBuf::from("/home/host/.config")
    );
    assert_eq!(
        expand_bind_source("${DECK_ROOT}/.config", &variables()).unwrap(),
        PathBuf::from("/work/deck/.config")
    );
    assert_eq!(
        expand_bind_target("${HOME}/.config").unwrap(),
        "${HOME}/.config"
    );
    assert!(expand_bind_source("${HOME}/.config", &variables()).is_err());
    assert!(expand_bind_target("$HOME/.config").is_err());
    assert!(expand_bind_target("${HOST_HOME}/.config").is_err());
    assert!(expand_bind_target("${HOME}/../.config").is_err());
}

#[test]
fn resolved_bind_rejects_parent_traversal_in_declared_or_expanded_source() {
    let root = std::env::temp_dir().join(format!("dembly-bind-target-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".dembly")).unwrap();
    fs::write(root.join("host-file"), "host").unwrap();
    fs::write(
        root.join(".dembly/config.toml"),
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[binds]]\nsource = \"${DECK_ROOT}/../host-file\"\ntarget = \"${HOME}/.config\"\nmode = \"ro\"\n",
    )
    .unwrap();
    let config = root.join(".dembly/config.toml");
    assert!(resolve_deck(&config, &variables_for(&config))
        .unwrap_err()
        .to_string()
        .contains("Host Bind source must not contain parent traversal"));

    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[binds]]\nsource = \"${HOST_HOME}/../host-file\"\ntarget = \"${HOME}/.config\"\nmode = \"ro\"\n",
    )
    .unwrap();
    assert!(resolve_deck(
        &config,
        &HostVariables {
            host_home: root.join("host-home"),
            deck_root: root.join(".dembly"),
        },
    )
    .unwrap_err()
    .to_string()
    .contains("Host Bind source must not contain parent traversal"));

    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[binds]]\nsource = \"/tmp/dembly/../host-file\"\ntarget = \"${HOME}/.config\"\nmode = \"ro\"\n",
    )
    .unwrap();
    assert!(resolve_deck(&config, &variables_for(&config))
        .unwrap_err()
        .to_string()
        .contains("Host Bind source must not contain parent traversal"));
}

#[test]
fn resolved_bind_keeps_its_runtime_target() {
    let root =
        std::env::temp_dir().join(format!("dembly-bind-runtime-target-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".dembly")).unwrap();
    fs::write(root.join(".dembly/host-file"), "host").unwrap();
    let config = root.join(".dembly/config.toml");
    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[binds]]\nsource = \"${DECK_ROOT}/host-file\"\ntarget = \"${HOME}/.config\"\nmode = \"ro\"\n",
    )
    .unwrap();
    let resolved = resolve_deck(&config, &variables_for(&config)).unwrap();
    assert_eq!(resolved.binds[0].target, "${HOME}/.config");
}

#[test]
fn resolved_volume_rejects_symlinked_storage_components() {
    let root = std::env::temp_dir().join(format!("dembly-volume-symlinks-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".dembly")).unwrap();
    let config = root.join(".dembly/config.toml");
    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[volumes]]\nname = \"build\"\ntarget = \"/workspace/build\"\n",
    )
    .unwrap();

    fs::create_dir_all(root.join("volume-target")).unwrap();
    std::os::unix::fs::symlink(root.join("volume-target"), root.join(".dembly/volumes")).unwrap();
    assert!(resolve_deck(&config, &variables_for(&config)).is_err());

    let card_root = root.join("card-intermediate");
    fs::create_dir_all(card_root.join(".dembly/cards/clang")).unwrap();
    let card_config = card_root.join(".dembly/config.toml");
    fs::write(
        card_root.join(".dembly/cards/clang/card.toml"),
        "schema_version = 1\nname = \"clang\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n[mount]\ntarget = \"/opt/clang\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/cache\"\n",
    )
    .unwrap();
    fs::write(
        &card_config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"cards/clang/card.toml\"\n",
    )
    .unwrap();
    fs::create_dir_all(card_root.join(".dembly/volumes")).unwrap();
    fs::create_dir_all(card_root.join("volume-target")).unwrap();
    assert!(resolve_deck(&card_config, &variables_for(&card_config)).is_ok());
    std::os::unix::fs::symlink(
        card_root.join("volume-target"),
        card_root.join(".dembly/volumes/clang"),
    )
    .unwrap();
    assert!(resolve_deck(&card_config, &variables_for(&card_config)).is_err());
}

#[test]
fn runtime_export_and_volume_targets_reject_parent_traversal() {
    let root = std::env::temp_dir().join(format!(
        "dembly-runtime-target-traversal-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let card_root = root.join(".dembly/cards/tool");
    fs::create_dir_all(&card_root).unwrap();
    let config = root.join(".dembly/config.toml");
    let card = card_root.join("card.toml");

    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"cards/tool/card.toml\"\n",
    )
    .unwrap();
    fs::write(
        &card,
        "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n[mount]\ntarget = \"/opt/tool\"\n[[exports]]\nsource = \"bin/tool\"\ntarget = \"/usr/local/bin/../../../tmp/tool\"\n",
    )
    .unwrap();

    assert!(resolve_deck(&config, &variables_for(&config)).is_err());

    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[volumes]]\nname = \"cache\"\ntarget = \"/workspace/../../../tmp/cache\"\n",
    )
    .unwrap();
    assert!(resolve_deck(&config, &variables_for(&config)).is_err());
}

#[test]
fn runtime_volume_target_symlinks_are_not_resolved_against_the_host_filesystem() {
    let root = std::env::temp_dir().join(format!(
        "dembly-runtime-target-symlink-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".dembly")).unwrap();
    fs::create_dir_all(root.join("outside")).unwrap();
    let link = root.join("runtime-link");
    std::os::unix::fs::symlink(root.join("outside"), &link).unwrap();
    let config = root.join(".dembly/config.toml");
    fs::write(
        &config,
        format!(
            "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[volumes]]\nname = \"cache\"\ntarget = \"{}/cache\"\n",
            link.display()
        ),
    )
    .unwrap();

    let resolved = resolve_deck(&config, &variables_for(&config)).unwrap();
    assert_eq!(resolved.volumes[0].target, link.join("cache"));
}

#[test]
fn normalized_export_targets_are_rejected_as_duplicates() {
    let root = std::env::temp_dir().join(format!(
        "dembly-normalized-export-collision-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let card_root = root.join(".dembly/cards/tool");
    fs::create_dir_all(&card_root).unwrap();
    let config = root.join(".dembly/config.toml");
    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n[[cards]]\npath = \"cards/tool/card.toml\"\n",
    )
    .unwrap();
    fs::write(
        card_root.join("card.toml"),
        "schema_version = 1\nname = \"tool\"\nversion = \"1\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n[mount]\ntarget = \"/opt/tool\"\n[[exports]]\nsource = \"bin/one\"\ntarget = \"/usr/local/bin/tool\"\n[[exports]]\nsource = \"bin/two\"\ntarget = \"/usr/local/bin//tool\"\n",
    )
    .unwrap();

    let error = resolve_deck(&config, &variables_for(&config)).unwrap_err();
    assert!(
        error.to_string().contains("duplicate export target"),
        "{error}"
    );
}

#[test]
fn resolved_deck_exposes_config_relative_and_deck_root_compose_paths() {
    let root = std::env::temp_dir().join(format!("dembly-compose-path-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".dembly")).unwrap();
    fs::write(root.join("compose.yaml"), "services: {}\n").unwrap();
    fs::write(
        root.join(".dembly/config.toml"),
        "schema_version = 1\n[compose]\npath = \"../compose.yaml\"\nservice = \"dev\"\n",
    )
    .unwrap();
    let config = root.join(".dembly/config.toml");
    assert_eq!(
        resolve_deck(&config, &variables_for(&config))
            .unwrap()
            .compose_path,
        root.join("compose.yaml")
    );

    fs::write(root.join(".dembly/compose.yaml"), "services: {}\n").unwrap();
    fs::write(
        &config,
        "schema_version = 1\n[compose]\npath = \"${DECK_ROOT}/compose.yaml\"\nservice = \"dev\"\n",
    )
    .unwrap();
    assert_eq!(
        resolve_deck(&config, &variables_for(&config))
            .unwrap()
            .compose_path,
        root.join(".dembly/compose.yaml")
    );
}

fn variables_for(config: &std::path::Path) -> HostVariables {
    HostVariables {
        host_home: PathBuf::from("/home/host"),
        deck_root: config.parent().unwrap().to_path_buf(),
    }
}

#[test]
fn volume_layout_distinguishes_private_and_shared_card_volumes() {
    let root = PathBuf::from("/work/deck");
    assert_eq!(
        resolve_volume_path(&root, &VolumeOwner::Deck, "build", false).unwrap(),
        root.join("volumes/build")
    );
    assert_eq!(
        resolve_volume_path(&root, &VolumeOwner::Card("clang".into()), "cache", false).unwrap(),
        root.join("volumes/clang/cache")
    );
    assert_eq!(
        resolve_volume_path(&root, &VolumeOwner::Card("clang".into()), "cache", true).unwrap(),
        root.join("volumes/cache")
    );
}

#[test]
fn mixed_shared_volume_declarations_are_rejected() {
    let declarations = [("cache".to_owned(), true), ("cache".to_owned(), false)];
    assert!(validate_shared_volume_consistency(&declarations).is_err());
}

#[test]
fn exact_mount_target_collision_is_rejected_but_nested_targets_are_allowed() {
    let duplicate = [
        MountResource::new("card:clang", "/opt/clang"),
        MountResource::new("bind:config", "/opt/clang"),
    ];
    assert!(validate_mount_targets(&duplicate).is_err());

    let nested = [
        MountResource::new("card:clang", "/opt/clang"),
        MountResource::new("bind:config", "/opt/clang/config"),
    ];
    assert!(validate_mount_targets(&nested).is_ok());
}

#[test]
fn card_environment_overrides_deck_and_path_preserves_card_order() {
    let base = BTreeMap::from([
        ("MODE".into(), "base".into()),
        ("PATH".into(), "/usr/bin".into()),
    ]);
    let deck = BTreeMap::from([("MODE".into(), "deck".into())]);
    let cards = vec![
        CardEnvironment::new(
            "/opt/one",
            BTreeMap::from([("MODE".into(), "one".into())]),
            vec!["bin".into()],
        ),
        CardEnvironment::new(
            "/opt/two",
            BTreeMap::from([("TOOL".into(), "two".into())]),
            vec!["tools".into()],
        ),
    ];

    let environment = plan_environment(base, deck, &cards, &["/project/bin".into()]).unwrap();
    assert_eq!(environment.get("MODE"), Some(&"one".to_owned()));
    assert_eq!(
        environment.get("PATH"),
        Some(&"/opt/one/bin:/opt/two/tools:/project/bin:/usr/bin".to_owned())
    );
}

#[test]
fn duplicate_card_environment_keys_are_rejected() {
    let cards = vec![
        CardEnvironment::new(
            "/opt/one",
            BTreeMap::from([("TOOL".into(), "one".into())]),
            vec![],
        ),
        CardEnvironment::new(
            "/opt/two",
            BTreeMap::from([("TOOL".into(), "two".into())]),
            vec![],
        ),
    ];
    assert!(plan_environment(BTreeMap::new(), BTreeMap::new(), &cards, &[]).is_err());
}
