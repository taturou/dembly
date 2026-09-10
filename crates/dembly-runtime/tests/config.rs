use dembly_runtime::load_runtime_config;
use std::fs;

#[test]
fn runtime_config_reads_resolved_card_paths_without_deck_manifests() {
    let path = std::env::temp_dir().join(format!("dembly-runtime-{}.toml", std::process::id()));
    fs::write(
        &path,
        "schema_version = 1\ndeck_name = \"example\"\n[runtime_user]\nname = \"developer\"\nuid = 1000\ngid = 1000\nhome = \"/home/developer\"\n[[cards]]\nname = \"clang\"\nimage = \"/run/dembly/cards/clang.squashfs\"\nmount_target = \"/opt/dembly/cards/clang\"\n[process]\nargv = [\"/bin/sh\", \"-c\", \"sleep 1\"]\n",
    ).unwrap();

    let config = load_runtime_config(&path).unwrap();
    assert_eq!(
        config.cards[0].image.to_string_lossy(),
        "/run/dembly/cards/clang.squashfs"
    );
    assert_eq!(config.process_argv, ["/bin/sh", "-c", "sleep 1"]);
}
