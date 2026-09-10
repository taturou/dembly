use crate::CoreError;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindVariables {
    pub host_home: PathBuf,
    pub deck_root: PathBuf,
    pub user: String,
    pub home: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VolumeOwner {
    Deck,
    Card(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountResource {
    pub owner: String,
    pub target: PathBuf,
}

impl MountResource {
    pub fn new(owner: impl Into<String>, target: impl Into<PathBuf>) -> Self {
        Self { owner: owner.into(), target: target.into() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardEnvironment {
    pub mount_root: PathBuf,
    pub values: BTreeMap<String, String>,
    pub path_prepend: Vec<String>,
}

impl CardEnvironment {
    pub fn new(mount_root: impl Into<PathBuf>, values: BTreeMap<String, String>, path_prepend: Vec<String>) -> Self {
        Self { mount_root: mount_root.into(), values, path_prepend }
    }
}

pub fn expand_bind_source(value: &str, variables: &BindVariables) -> Result<PathBuf, CoreError> {
    expand(value, &[ ("HOST_HOME", variables.host_home.to_string_lossy().as_ref()), ("DECK_ROOT", variables.deck_root.to_string_lossy().as_ref()) ])
        .map(PathBuf::from)
}

pub fn expand_bind_target(value: &str, variables: &BindVariables) -> Result<PathBuf, CoreError> {
    expand(value, &[ ("USER", variables.user.as_str()), ("HOME", variables.home.as_str()) ]).map(PathBuf::from)
}

pub fn resolve_volume_path(deck_root: &Path, owner: &VolumeOwner, name: &str, shared: bool) -> Result<PathBuf, CoreError> {
    if name.is_empty() || !name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_') {
        return Err(domain_error(format!("invalid Volume name: {name}")));
    }
    let base = deck_root.join("volumes");
    Ok(if shared {
        base.join(name)
    } else {
        match owner {
            VolumeOwner::Deck => base.join(name),
            VolumeOwner::Card(card_name) => base.join(card_name).join(name),
        }
    })
}

pub fn validate_shared_volume_consistency(declarations: &[(String, bool)]) -> Result<(), CoreError> {
    let mut states = BTreeMap::new();
    for (name, shared) in declarations {
        if let Some(previous) = states.insert(name, shared) {
            if previous != shared {
                return Err(domain_error(format!("Volume {name} mixes shared=true and shared=false")));
            }
        }
    }
    Ok(())
}

pub fn validate_mount_targets(resources: &[MountResource]) -> Result<(), CoreError> {
    let mut seen = BTreeMap::new();
    for resource in resources {
        if !resource.target.is_absolute() {
            return Err(domain_error(format!("mount target must be absolute: {}", resource.target.display())));
        }
        if let Some(previous) = seen.insert(&resource.target, &resource.owner) {
            return Err(domain_error(format!("mount target collision at {} between {previous} and {}", resource.target.display(), resource.owner)));
        }
    }
    Ok(())
}

pub fn plan_environment(
    mut base: BTreeMap<String, String>,
    deck: BTreeMap<String, String>,
    cards: &[CardEnvironment],
    deck_path_prepend: &[String],
) -> Result<BTreeMap<String, String>, CoreError> {
    let base_path = base.get("PATH").cloned().unwrap_or_default();
    base.extend(deck);
    let mut card_keys = BTreeSet::new();
    let mut path_entries = Vec::new();
    for card in cards {
        for key in card.values.keys() {
            if !card_keys.insert(key) {
                return Err(domain_error(format!("Card environment conflict: {key}")));
            }
        }
        base.extend(card.values.clone());
        for entry in &card.path_prepend {
            let path = Path::new(entry);
            path_entries.push(if path.is_absolute() { path.to_path_buf() } else { card.mount_root.join(path) }.to_string_lossy().into_owned());
        }
    }
    path_entries.extend(deck_path_prepend.iter().cloned());
    if !base_path.is_empty() { path_entries.push(base_path); }
    if !path_entries.is_empty() { base.insert("PATH".into(), path_entries.join(":")); }
    Ok(base)
}

fn expand(value: &str, bindings: &[(&str, &str)]) -> Result<String, CoreError> {
    let mut result = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find('$') {
        result.push_str(&remaining[..start]);
        let suffix = &remaining[start..];
        let Some(close) = suffix.find('}') else { return Err(domain_error(format!("invalid variable syntax: {value}"))); };
        if !suffix.starts_with("${") { return Err(domain_error(format!("invalid variable syntax: {value}"))); }
        let name = &suffix[2..close];
        let replacement = bindings.iter().find_map(|(candidate, replacement)| (*candidate == name).then_some(*replacement))
            .ok_or_else(|| domain_error(format!("undefined bind variable: {name}")))?;
        result.push_str(replacement);
        remaining = &suffix[close + 1..];
    }
    result.push_str(remaining);
    Ok(result)
}

fn domain_error(message: String) -> CoreError {
    CoreError::parse("<resolved deck>", message)
}
