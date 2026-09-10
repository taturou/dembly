use crate::{CardDocument, CoreError};
use std::path::Path;
use std::process::Command;

pub fn verify_card_filesystem(manifest_path: &Path, card: &CardDocument) -> Result<(), CoreError> {
    if card.filesystem.file.is_empty() || Path::new(&card.filesystem.file).is_absolute() {
        return Err(CoreError::parse(
            manifest_path,
            "filesystem.file must be a Card-root relative path",
        ));
    }
    let card_root = manifest_path
        .parent()
        .ok_or_else(|| CoreError::parse(manifest_path, "card manifest has no parent directory"))?;
    let artifact = card_root.join(&card.filesystem.file);
    if !artifact.is_file() {
        return Err(CoreError::parse(
            manifest_path,
            format!("missing SquashFS: {}", artifact.display()),
        ));
    }
    let output = Command::new("sha256sum")
        .arg(&artifact)
        .output()
        .map_err(|error| {
            CoreError::parse(manifest_path, format!("cannot calculate checksum: {error}"))
        })?;
    if !output.status.success() {
        return Err(CoreError::parse(
            manifest_path,
            format!("checksum command failed: {}", output.status),
        ));
    }
    let actual = String::from_utf8(output.stdout)
        .map_err(|_| CoreError::parse(manifest_path, "checksum command emitted non-UTF-8 output"))?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    if actual != card.filesystem.sha256 {
        return Err(CoreError::parse(
            manifest_path,
            format!(
                "checksum mismatch: expected {}, got {actual}",
                card.filesystem.sha256
            ),
        ));
    }
    Ok(())
}
