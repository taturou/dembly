use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeCard {
    pub name: String,
    pub image: PathBuf,
    pub mount_target: PathBuf,
}

pub fn squashfs_mount_command(card: &RuntimeCard) -> Vec<String> {
    vec![
        "mount".into(),
        "-t".into(),
        "squashfs".into(),
        "-o".into(),
        "loop,ro".into(),
        card.image.to_string_lossy().into_owned(),
        card.mount_target.to_string_lossy().into_owned(),
    ]
}

pub fn mount_card(card: &RuntimeCard) -> Result<(), String> {
    std::fs::create_dir_all(&card.mount_target).map_err(|error| format!("cannot create {}: {error}", card.mount_target.display()))?;
    let command = squashfs_mount_command(card);
    let status = Command::new(&command[0]).args(&command[1..]).status().map_err(|error| format!("cannot execute mount: {error}"))?;
    if status.success() { Ok(()) } else { Err(format!("SquashFS mount failed for {} with status {status}", card.name)) }
}
