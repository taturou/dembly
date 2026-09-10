use crate::RuntimeCard;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub schema_version: u32,
    pub deck_name: String,
    pub runtime_user: RuntimeUser,
    pub cards: Vec<RuntimeCard>,
    pub exports: Vec<RuntimeExport>,
    pub hooks: Vec<RuntimeHook>,
    pub environment: BTreeMap<String, String>,
    pub process_argv: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeExport {
    pub source: PathBuf,
    pub target: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHook {
    pub card: String,
    pub exec: PathBuf,
    pub args: Vec<String>,
}

pub fn load_runtime_config(path: &Path) -> Result<RuntimeConfig, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("cannot read runtime config {}: {error}", path.display()))?;
    let mut root: BTreeMap<String, String> = BTreeMap::new();
    let mut user: BTreeMap<String, String> = BTreeMap::new();
    let mut process: BTreeMap<String, String> = BTreeMap::new();
    let mut cards = Vec::new();
    let mut card: Option<BTreeMap<String, String>> = None;
    let mut exports = Vec::new();
    let mut export: Option<BTreeMap<String, String>> = None;
    let mut hooks = Vec::new();
    let mut hook: Option<BTreeMap<String, String>> = None;
    let mut environment = BTreeMap::new();
    let mut section = "root";
    for raw in content.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        match line {
            "[runtime_user]" => {
                section = "user";
                continue;
            }
            "[process]" => {
                section = "process";
                continue;
            }
            "[environment]" => {
                section = "environment";
                continue;
            }
            "[[cards]]" => {
                if let Some(previous) = card.take() {
                    cards.push(previous);
                }
                card = Some(BTreeMap::new());
                section = "cards";
                continue;
            }
            "[[exports]]" => {
                if let Some(previous) = export.take() {
                    exports.push(previous);
                }
                export = Some(BTreeMap::new());
                section = "exports";
                continue;
            }
            "[[hooks]]" => {
                if let Some(previous) = hook.take() {
                    hooks.push(previous);
                }
                hook = Some(BTreeMap::new());
                section = "hooks";
                continue;
            }
            _ => {}
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid runtime.toml entry: {line}"))?;
        let value = value.trim().to_owned();
        match section {
            "root" => {
                root.insert(key.trim().into(), unquote(&value));
            }
            "user" => {
                user.insert(key.trim().into(), unquote(&value));
            }
            "process" => {
                process.insert(key.trim().into(), value);
            }
            "cards" => {
                card.as_mut()
                    .ok_or("Card entry before [[cards]]")?
                    .insert(key.trim().into(), unquote(&value));
            }
            "exports" => {
                export
                    .as_mut()
                    .ok_or("Export entry before [[exports]]")?
                    .insert(key.trim().into(), unquote(&value));
            }
            "hooks" => {
                hook.as_mut()
                    .ok_or("Hook entry before [[hooks]]")?
                    .insert(key.trim().into(), unquote(&value));
            }
            "environment" => {
                environment.insert(key.trim().into(), unquote(&value));
            }
            _ => unreachable!(),
        }
    }
    if let Some(card) = card {
        cards.push(card);
    }
    if let Some(export) = export {
        exports.push(export);
    }
    if let Some(hook) = hook {
        hooks.push(hook);
    }
    let cards = cards
        .into_iter()
        .map(|card| {
            Ok(RuntimeCard {
                name: required(&card, "name")?,
                image: PathBuf::from(required(&card, "image")?),
                mount_target: PathBuf::from(required(&card, "mount_target")?),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let exports = exports
        .into_iter()
        .map(|entry| {
            Ok(RuntimeExport {
                source: PathBuf::from(required(&entry, "source")?),
                target: PathBuf::from(required(&entry, "target")?),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let hooks = hooks
        .into_iter()
        .map(|entry| {
            Ok(RuntimeHook {
                card: required(&entry, "card")?,
                exec: PathBuf::from(required(&entry, "exec")?),
                args: string_array(entry.get("args").map(String::as_str).unwrap_or("[]"))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(RuntimeConfig {
        schema_version: required(&root, "schema_version")?
            .parse()
            .map_err(|_| "runtime schema_version must be numeric".to_owned())?,
        deck_name: required(&root, "deck_name")?,
        runtime_user: RuntimeUser {
            name: required(&user, "name")?,
            uid: required(&user, "uid")?
                .parse()
                .map_err(|_| "runtime uid must be numeric".to_owned())?,
            gid: required(&user, "gid")?
                .parse()
                .map_err(|_| "runtime gid must be numeric".to_owned())?,
            home: required(&user, "home")?,
        },
        cards,
        exports,
        hooks,
        environment,
        process_argv: string_array(
            process
                .get("argv")
                .ok_or("runtime process argv is required")?,
        )?,
    })
}

fn required(values: &BTreeMap<String, String>, key: &str) -> Result<String, String> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| format!("runtime config missing {key}"))
}
fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}
fn string_array(value: &str) -> Result<Vec<String>, String> {
    let inner = value
        .trim()
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or("runtime argv must be an array")?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|value| {
            let value = value.trim();
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_owned)
                .ok_or_else(|| "runtime argv entries must be strings".to_owned())
        })
        .collect()
}
