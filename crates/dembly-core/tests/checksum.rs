use dembly_core::{load_card, verify_card_filesystem};
use std::fs;

#[test]
fn checksum_verification_accepts_matching_file_and_rejects_mismatch() {
    let directory = std::env::temp_dir().join(format!("dembly-checksum-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("rootfs.squashfs"), b"abc").unwrap();
    let manifest = directory.join("card.toml");
    fs::write(
        &manifest,
        "schema_version = 1\nname = \"clang\"\nversion = \"20\"\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\n[mount]\ntarget = \"/opt/clang\"\n",
    ).unwrap();
    let card = load_card(&manifest).unwrap();
    verify_card_filesystem(&manifest, &card).unwrap();

    fs::write(&manifest, fs::read_to_string(&manifest).unwrap().replace("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "0000000000000000000000000000000000000000000000000000000000000000")).unwrap();
    let card = load_card(&manifest).unwrap();
    assert!(verify_card_filesystem(&manifest, &card).unwrap_err().to_string().contains("checksum mismatch"));
}
