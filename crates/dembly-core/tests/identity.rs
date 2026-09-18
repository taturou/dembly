use dembly_core::{sha256_bytes, CardIdentity, LockInput};

fn lock(cards: Vec<CardIdentity>) -> LockInput {
    LockInput {
        compose_path: "/work/project/compose.yaml".into(),
        service: "dev".into(),
        image: "sha256:image".into(),
        cards,
    }
}

fn card(name: &str, manifest: &str, filesystem: &str) -> CardIdentity {
    CardIdentity {
        name: name.into(),
        version: "1".into(),
        manifest_sha256: manifest.into(),
        filesystem_sha256: filesystem.into(),
    }
}

#[test]
fn digest_preserves_card_configuration_order() {
    let alpha = card("alpha", "manifest-alpha", "filesystem-alpha");
    let beta = card("beta", "manifest-beta", "filesystem-beta");

    assert_ne!(
        lock(vec![alpha.clone(), beta.clone()]).digest(),
        lock(vec![beta, alpha]).digest(),
        "reordering configured Cards must identify a different Lock input"
    );
}

#[test]
fn digest_is_independent_of_serialized_map_order() {
    let first: LockInput = serde_json::from_str(
        r#"{
            "compose_path":"/work/project/compose.yaml",
            "service":"dev",
            "image":"sha256:image",
            "cards":[{
                "name":"tool",
                "version":"1",
                "manifest_sha256":"manifest",
                "filesystem_sha256":"filesystem"
            }]
        }"#,
    )
    .unwrap();
    let reordered: LockInput = serde_json::from_str(
        r#"{
            "cards":[{
                "filesystem_sha256":"filesystem",
                "manifest_sha256":"manifest",
                "version":"1",
                "name":"tool"
            }],
            "image":"sha256:image",
            "service":"dev",
            "compose_path":"/work/project/compose.yaml"
        }"#,
    )
    .unwrap();

    assert_eq!(first.digest(), reordered.digest());
}

#[test]
fn digest_changes_with_manifest_or_verified_filesystem_identity() {
    let original = lock(vec![card("tool", "manifest-one", "filesystem-one")]);
    let changed_manifest = lock(vec![card("tool", "manifest-two", "filesystem-one")]);
    let changed_filesystem = lock(vec![card("tool", "manifest-one", "filesystem-two")]);

    assert_ne!(original.digest(), changed_manifest.digest());
    assert_ne!(original.digest(), changed_filesystem.digest());
    assert_eq!(
        original.digest(),
        "sha256:a5ed02ded110315527b40780c269c38c36b0a26d45cea8689d4141c2634375af"
    );
}

#[test]
fn digest_uses_the_shared_byte_checksum() {
    let canonical = concat!(
        "dembly-lock-v1\0",
        "26:/work/project/compose.yaml\0",
        "3:dev\0",
        "12:sha256:image\0",
        "4:tool\0",
        "1:1\0",
        "12:manifest-one\0",
        "14:filesystem-one\0",
    );
    let input = lock(vec![card("tool", "manifest-one", "filesystem-one")]);

    assert_eq!(
        input.digest(),
        format!("sha256:{}", sha256_bytes(canonical.as_bytes()))
    );
}

#[test]
fn digest_does_not_include_managed_compose_document_bytes() {
    let compose_path =
        std::env::temp_dir().join(format!("dembly-identity-compose-{}", std::process::id()));
    std::fs::write(
        &compose_path,
        b"name: project\nservices: {dev: {image: first}}\n",
    )
    .unwrap();
    let mut input = lock(vec![card("tool", "manifest", "filesystem")]);
    input.compose_path = compose_path.display().to_string();
    let first = input.digest();

    std::fs::write(
        &compose_path,
        b"name: project\nservices:\n  dev:\n    image: second\n",
    )
    .unwrap();
    let second = input.digest();
    std::fs::remove_file(compose_path).unwrap();

    let before = b"name: project\nservices: {dev: {image: first}}\n";
    let after = b"name: project\nservices:\n  dev:\n    image: second\n";
    assert_ne!(before.as_slice(), after.as_slice());
    assert_eq!(first, second);
}
