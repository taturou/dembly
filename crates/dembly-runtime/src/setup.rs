use crate::{ResolvedRuntimeUser, RuntimeConfig, RuntimeExport, RuntimeHook};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub fn create_exports(exports: &[RuntimeExport]) -> Result<(), String> {
    for export in exports {
        if let Some(parent) = export.target.parent() {
            ensure_directory_components(parent, true, "export parent")?;
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
        ensure_directory_components(target, false, &format!("Compose Volume {index} target"))?;
    }
    Ok(())
}

fn ensure_directory_components(path: &Path, create: bool, label: &str) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(format!(
            "{label} must be an absolute path without parent traversal: {}",
            path.display()
        ));
    }

    let mut current = PathBuf::from("/");
    for component in path.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        current.push(component);
        loop {
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(format!(
                        "{label} must not traverse a symbolic link: {}",
                        current.display()
                    ));
                }
                Ok(metadata) if metadata.is_dir() => break,
                Ok(_) => {
                    return Err(format!(
                        "{label} component is not a directory: {}",
                        current.display()
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                    match fs::create_dir(&current) {
                        Ok(()) => continue,
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(error) => {
                            return Err(format!(
                                "cannot create {label} {}: {error}",
                                current.display()
                            ));
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(format!(
                        "{label} is not present as a directory: {}",
                        current.display()
                    ));
                }
                Err(error) => {
                    return Err(format!(
                        "cannot inspect {label} {}: {error}",
                        current.display()
                    ));
                }
            }
        }
    }
    Ok(())
}
