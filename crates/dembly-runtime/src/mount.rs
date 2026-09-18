use crate::{ResolvedRuntimeUser, RuntimeBind};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
    fs::create_dir_all(&card.mount_target)
        .map_err(|error| format!("cannot create {}: {error}", card.mount_target.display()))?;
    let command = squashfs_mount_command(card);
    run_command(&command).map_err(|error| {
        format!(
            "SquashFS mount failed for Card {} (source {}, target {}): {error}",
            card.name,
            card.image.display(),
            card.mount_target.display()
        )
    })
}

pub fn expand_runtime_bind_target(
    target: &str,
    user: &ResolvedRuntimeUser,
) -> Result<PathBuf, String> {
    let mut expanded = String::new();
    let mut remaining = target;
    while let Some(start) = remaining.find('$') {
        expanded.push_str(&remaining[..start]);
        let suffix = &remaining[start..];
        let close = suffix
            .find('}')
            .filter(|_| suffix.starts_with("${"))
            .ok_or_else(|| format!("invalid Runtime Bind target variable syntax: {target}"))?;
        let value = match &suffix[2..close] {
            "HOME" => &user.home,
            "USER" => &user.name,
            name => {
                return Err(format!(
                    "unsupported Runtime Bind target variable {name}: {target}"
                ))
            }
        };
        expanded.push_str(value);
        remaining = &suffix[close + 1..];
    }
    expanded.push_str(remaining);
    let path = PathBuf::from(expanded);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(format!(
            "Runtime Bind target must be absolute without parent traversal: {target}"
        ));
    }
    Ok(path)
}

pub fn mount_bind(bind: &RuntimeBind, user: &ResolvedRuntimeUser) -> Result<(), String> {
    let index = bind
        .source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.bytes().all(|byte| byte.is_ascii_digit()))
        .unwrap_or("?");
    let target = expand_runtime_bind_target(&bind.target, user).map_err(|error| {
        bind_error(index, bind, Path::new(&bind.target), "expand target", error)
    })?;
    if bind.mode != "ro" && bind.mode != "rw" {
        return Err(bind_error(
            index,
            bind,
            &target,
            "validate mode",
            format!("mode must be ro or rw, got {}", bind.mode),
        ));
    }
    let metadata = fs::metadata(&bind.source)
        .map_err(|error| bind_error(index, bind, &target, "inspect staged source", error))?;
    if metadata.is_dir() {
        fs::create_dir_all(&target)
            .map_err(|error| bind_error(index, bind, &target, "create directory target", error))?;
    } else if metadata.is_file() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                bind_error(index, bind, &target, "create file target parent", error)
            })?;
        }
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&target)
            .map_err(|error| bind_error(index, bind, &target, "create file target", error))?;
    } else {
        return Err(bind_error(
            index,
            bind,
            &target,
            "inspect staged source",
            "source is neither a directory nor a regular file",
        ));
    }

    run_command(&[
        "mount".into(),
        "--bind".into(),
        bind.source.to_string_lossy().into_owned(),
        target.to_string_lossy().into_owned(),
    ])
    .map_err(|error| bind_error(index, bind, &target, "bind mount", error))?;

    if bind.mode == "ro" {
        run_command(&[
            "mount".into(),
            "-o".into(),
            "remount,bind,ro".into(),
            target.to_string_lossy().into_owned(),
        ])
        .map_err(|error| bind_error(index, bind, &target, "read-only remount", error))?;
    }
    Ok(())
}

fn run_command(command: &[String]) -> Result<(), String> {
    let status = Command::new(&command[0])
        .args(&command[1..])
        .status()
        .map_err(|error| format!("cannot execute {}: {error}", command[0]))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with status {status}", command[0]))
    }
}

fn bind_error(
    index: &str,
    bind: &RuntimeBind,
    target: &Path,
    operation: &str,
    error: impl std::fmt::Display,
) -> String {
    format!(
        "Host Bind {index} (source {}, target {}) failed to {operation}: {error}",
        bind.source.display(),
        target.display()
    )
}
