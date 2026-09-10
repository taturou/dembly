use dembly_core::{
    expand_bind_source, expand_bind_target, plan_environment, resolve_volume_path,
    validate_mount_targets, validate_shared_volume_consistency, BindVariables, CardEnvironment,
    MountResource, VolumeOwner,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn variables() -> BindVariables {
    BindVariables {
        host_home: PathBuf::from("/home/host"),
        deck_root: PathBuf::from("/work/deck"),
        user: "developer".into(),
        home: "/home/developer".into(),
    }
}

#[test]
fn bind_variables_are_scoped_by_source_and_target() {
    assert_eq!(
        expand_bind_source("${HOST_HOME}/.config", &variables()).unwrap(),
        PathBuf::from("/home/host/.config")
    );
    assert_eq!(
        expand_bind_target("${HOME}/.config", &variables()).unwrap(),
        PathBuf::from("/home/developer/.config")
    );
    assert!(expand_bind_source("${HOME}/.config", &variables()).is_err());
    assert!(expand_bind_target("$HOME/.config", &variables()).is_err());
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
