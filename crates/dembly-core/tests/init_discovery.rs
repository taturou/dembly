use dembly_core::discover_init_candidates;
use std::fs;
use std::path::{Path, PathBuf};

fn temporary_directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "dembly-init-discovery-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn card(name: &str) -> String {
    format!(
        "schema_version = 1\nname = \"{name}\"\nversion = \"1.0.0\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"abc\"\n[mount]\ntarget = \"/opt/dembly/cards/{name}\"\n"
    )
}

#[test]
fn discovers_only_direct_regular_files_in_fixed_roots_in_lexical_order() {
    let cwd = temporary_directory("fixed-roots");
    write(
        &cwd.join(".dembly/cards/zulu/card.toml"),
        &card("duplicate"),
    );
    write(&cwd.join(".dembly-cards/bravo/card.toml"), &card("bravo"));
    write(&cwd.join(".cards/alpha/card.toml"), &card("duplicate"));
    write(&cwd.join("cards/charlie/card.toml"), &card("charlie"));
    write(
        &cwd.join(".dembly/cards/zulu/nested/card.toml"),
        &card("nested"),
    );
    write(&cwd.join("cardboard/outside/card.toml"), &card("outside"));
    write(
        &cwd.join(".dembly-cards-extra/outside/card.toml"),
        &card("outside"),
    );

    write(&cwd.join(".devcontainer.json"), "{}\n");
    write(&cwd.join(".devcontainer/devcontainer.json"), "{}\n");
    write(&cwd.join(".devcontainer/profile/devcontainer.json"), "{}\n");
    write(
        &cwd.join(".devcontainer/profile/deep/devcontainer.json"),
        "{}\n",
    );
    write(&cwd.join("devcontainer/devcontainer.json"), "{}\n");

    let candidates = discover_init_candidates(&cwd).unwrap();

    assert_eq!(
        candidates.cards,
        vec![
            dembly_core::CardCandidate {
                path: PathBuf::from(".cards/alpha/card.toml"),
                name: "duplicate".into(),
            },
            dembly_core::CardCandidate {
                path: PathBuf::from(".dembly-cards/bravo/card.toml"),
                name: "bravo".into(),
            },
            dembly_core::CardCandidate {
                path: PathBuf::from(".dembly/cards/zulu/card.toml"),
                name: "duplicate".into(),
            },
            dembly_core::CardCandidate {
                path: PathBuf::from("cards/charlie/card.toml"),
                name: "charlie".into(),
            },
        ]
    );
    assert_eq!(
        candidates.devcontainers,
        vec![
            PathBuf::from(".devcontainer.json"),
            PathBuf::from(".devcontainer/devcontainer.json"),
            PathBuf::from(".devcontainer/profile/devcontainer.json"),
        ]
    );
}

#[cfg(unix)]
#[test]
fn excludes_symlinked_directories_and_candidate_files() {
    use std::os::unix::fs::symlink;

    let cwd = temporary_directory("symlinks");
    write(&cwd.join("targets/card/card.toml"), &card("target"));
    write(&cwd.join("targets/devcontainer.json"), "{}\n");
    fs::create_dir_all(cwd.join(".dembly/cards")).unwrap();
    fs::create_dir_all(cwd.join(".devcontainer")).unwrap();
    symlink(
        cwd.join("targets/card"),
        cwd.join(".dembly/cards/link-card"),
    )
    .unwrap();
    fs::create_dir_all(cwd.join(".dembly/cards/link-file")).unwrap();
    symlink(
        cwd.join("targets/card/card.toml"),
        cwd.join(".dembly/cards/link-file/card.toml"),
    )
    .unwrap();
    symlink(
        cwd.join("targets"),
        cwd.join(".devcontainer/link-directory"),
    )
    .unwrap();
    symlink(
        cwd.join("targets/devcontainer.json"),
        cwd.join(".devcontainer/devcontainer.json"),
    )
    .unwrap();

    let candidates = discover_init_candidates(&cwd).unwrap();

    assert!(candidates.cards.is_empty());
    assert!(candidates.devcontainers.is_empty());
}
