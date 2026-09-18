use crate::{ResolvedRuntimeUser, RuntimeConfig, RuntimeExport, RuntimeHook};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn create_exports(exports: &[RuntimeExport]) -> Result<(), String> {
    for export in exports {
        if let Some(parent) = export.target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("cannot create export parent {}: {error}", parent.display())
            })?;
        }
        match fs::symlink_metadata(&export.target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let current = fs::read_link(&export.target).map_err(|error| {
                    format!("cannot read export {}: {error}", export.target.display())
                })?;
                if current != export.source {
                    return Err(format!(
                        "export target is occupied by a non-Dembly symlink: {}",
                        export.target.display()
                    ));
                }
            }
            Ok(_) => {
                return Err(format!(
                    "export target is occupied by a non-Dembly file: {}",
                    export.target.display()
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::os::unix::fs::symlink(&export.source, &export.target).map_err(|error| {
                    format!("cannot create export {}: {error}", export.target.display())
                })?
            }
            Err(error) => {
                return Err(format!(
                    "cannot inspect export target {}: {error}",
                    export.target.display()
                ))
            }
        }
    }
    Ok(())
}

pub fn run_hooks(
    hooks: &[RuntimeHook],
    cards: &[crate::RuntimeCard],
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    for hook in hooks {
        let card = cards
            .iter()
            .find(|card| card.name == hook.card)
            .ok_or_else(|| format!("hook references unknown Card: {}", hook.card))?;
        let status = Command::new(&hook.exec)
            .args(&hook.args)
            .current_dir(&card.mount_target)
            .envs(environment)
            .status()
            .map_err(|error| {
                format!(
                    "cannot execute post_mount hook {} for Card {}: {error}",
                    hook.exec.display(),
                    hook.card
                )
            })?;
        if !status.success() {
            return Err(format!(
                "post_mount hook {} for Card {} failed with status {status}",
                hook.exec.display(),
                hook.card
            ));
        }
    }
    Ok(())
}

pub fn run_checks(config: &RuntimeConfig, user: &ResolvedRuntimeUser) -> Result<(), String> {
    for check in &config.checks {
        if !config.cards.iter().any(|card| card.name == check.card) {
            return Err(format!("check references unknown Card: {}", check.card));
        }
    }

    let mut environment = config.environment.clone();
    environment
        .entry("HOME".into())
        .or_insert_with(|| user.home.clone());
    environment
        .entry("USER".into())
        .or_insert_with(|| user.name.clone());

    for card in &config.cards {
        for check in config.checks.iter().filter(|check| check.card == card.name) {
            let status = Command::new(&check.exec)
                .args(&check.args)
                .current_dir(&card.mount_target)
                .env_clear()
                .envs(&environment)
                .status()
                .map_err(|error| {
                    format!(
                        "cannot execute check {} for Card {}: {error}",
                        check.exec.display(),
                        check.card
                    )
                })?;
            if !status.success() {
                return Err(format!(
                    "check {} for Card {} failed with status {status}",
                    check.exec.display(),
                    check.card
                ));
            }
        }
    }
    Ok(())
}

pub fn ensure_mount_targets_exist(cards: &[crate::RuntimeCard]) -> Result<(), String> {
    for card in cards {
        if !card.mount_target.is_dir() {
            return Err(format!(
                "Card mount target does not exist after mount: {}",
                card.mount_target.display()
            ));
        }
    }
    Ok(())
}

pub fn ensure_volume_targets_exist(targets: &[PathBuf]) -> Result<(), String> {
    for (index, target) in targets.iter().enumerate() {
        if !Path::new(target).is_dir() {
            return Err(format!(
                "Compose Volume {index} target is not present as a directory: {}",
                target.display()
            ));
        }
    }
    Ok(())
}
