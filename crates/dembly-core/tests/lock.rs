use dembly_core::{read_lock, write_lock, DeckLock, LockBase, LockedCard};
use std::fs;

#[test]
fn lock_writer_round_trips_image_and_card_identities() {
    let path = std::env::temp_dir().join(format!("dembly-lock-{}.toml", std::process::id()));
    let lock = DeckLock {
        schema_version: 1,
        base: LockBase::Image {
            reference: "alpine:3.20".into(),
            resolved_image_id: "sha256:image".into(),
        },
        cards: vec![LockedCard {
            name: "clang".into(),
            version: "20".into(),
            source: "cards/clang/card.toml".into(),
            manifest_sha256: "sha256:manifest".into(),
            filesystem_sha256: "sha256:filesystem".into(),
        }],
    };
    write_lock(&path, &lock).unwrap();
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("resolved_image_id"));
    assert_eq!(read_lock(&path).unwrap(), lock);
}

#[test]
fn lock_writer_round_trips_compose_identity() {
    let path =
        std::env::temp_dir().join(format!("dembly-compose-lock-{}.toml", std::process::id()));
    let lock = DeckLock {
        schema_version: 1,
        base: LockBase::Compose {
            compose: "compose.yaml".into(),
            service: "dev".into(),
            compose_sha256: "sha256:compose".into(),
            resolved_image_id: "sha256:image".into(),
        },
        cards: Vec::new(),
    };
    write_lock(&path, &lock).unwrap();
    assert_eq!(read_lock(&path).unwrap(), lock);
}
