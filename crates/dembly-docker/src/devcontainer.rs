use serde::de::Deserializer;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevContainerDocument {
    pub docker_compose_file: Vec<String>,
    pub service: String,
    pub initialize_command: Option<DevContainerCommand>,
    pub override_command: Option<bool>,
    pub container_user: Option<String>,
    pub remote_user: String,
    pub update_remote_user_uid: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DevContainerCommand {
    String(String),
    Array(Vec<String>),
    Object(BTreeMap<String, DevContainerCommandArguments>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DevContainerCommandArguments {
    String(String),
    Array(Vec<String>),
}

#[derive(Deserialize)]
struct DevContainerWire {
    #[serde(
        rename = "dockerComposeFile",
        deserialize_with = "deserialize_compose_files"
    )]
    docker_compose_file: Vec<String>,
    service: String,
    #[serde(rename = "initializeCommand")]
    initialize_command: Option<DevContainerCommand>,
    #[serde(rename = "overrideCommand")]
    override_command: Option<bool>,
    #[serde(rename = "containerUser")]
    container_user: Option<String>,
    #[serde(rename = "remoteUser")]
    remote_user: String,
    #[serde(rename = "updateRemoteUserUID")]
    update_remote_user_uid: Option<bool>,
}

impl From<DevContainerWire> for DevContainerDocument {
    fn from(value: DevContainerWire) -> Self {
        Self {
            docker_compose_file: value.docker_compose_file,
            service: value.service,
            initialize_command: value.initialize_command,
            override_command: value.override_command,
            container_user: value.container_user,
            remote_user: value.remote_user,
            update_remote_user_uid: value.update_remote_user_uid,
        }
    }
}

impl<'de> Deserialize<'de> for DevContainerCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            String(String),
            Array(Vec<String>),
            Object(BTreeMap<String, DevContainerCommandArguments>),
        }
        match Wire::deserialize(deserializer)? {
            Wire::String(value) => Ok(Self::String(value)),
            Wire::Array(value) => Ok(Self::Array(value)),
            Wire::Object(value) => Ok(Self::Object(value)),
        }
    }
}

impl<'de> Deserialize<'de> for DevContainerCommandArguments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            String(String),
            Array(Vec<String>),
        }
        match Wire::deserialize(deserializer)? {
            Wire::String(value) => Ok(Self::String(value)),
            Wire::Array(value) => Ok(Self::Array(value)),
        }
    }
}

fn deserialize_compose_files<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wire {
        String(String),
        Array(Vec<String>),
    }
    match Wire::deserialize(deserializer)? {
        Wire::String(value) => Ok(vec![value]),
        Wire::Array(value) => Ok(value),
    }
}

pub fn load_devcontainer(path: &Path) -> Result<DevContainerDocument, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let wire: DevContainerWire = serde_json::from_str(&source)
        .map_err(|error| format!("invalid devcontainer.json {}: {error}", path.display()))?;
    Ok(wire.into())
}

pub fn validate_devcontainer(
    document: &DevContainerDocument,
    path: &Path,
    managed_compose: &Path,
    service: &str,
    intended_user: &str,
) -> Result<Vec<PathBuf>, String> {
    if document.service != service {
        return Err(format!(
            "devcontainer service {} does not match managed service {service}",
            document.service
        ));
    }
    if document.override_command == Some(true) {
        return Err("devcontainer overrideCommand must be false or omitted".into());
    }
    if document
        .container_user
        .as_deref()
        .is_some_and(|user| user != "root")
    {
        return Err("devcontainer containerUser must be root or omitted".into());
    }
    if document.remote_user != intended_user {
        return Err(format!(
            "devcontainer remoteUser {} does not match intended user {intended_user}",
            document.remote_user
        ));
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let compose_files = document
        .docker_compose_file
        .iter()
        .map(|value| normalize_path(&parent.join(value)))
        .collect::<Vec<_>>();
    let managed_compose = normalize_path(managed_compose);
    if compose_files.last() != Some(&managed_compose) {
        return Err(format!(
            "managed Compose file {} must be the last dockerComposeFile entry",
            managed_compose.display()
        ));
    }

    for compose in compose_files
        .iter()
        .filter(|compose| *compose != &managed_compose)
    {
        let source = fs::read_to_string(compose)
            .map_err(|error| format!("cannot read Compose file {}: {error}", compose.display()))?;
        if has_top_level_x_dembly(&source) {
            return Err(format!(
                "non-managed Compose file {} must not contain x-dembly",
                compose.display()
            ));
        }
    }
    Ok(compose_files)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push(component);
                }
            }
            Component::Normal(value) => normalized.push(value),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn has_top_level_x_dembly(source: &str) -> bool {
    source.lines().any(|line| {
        let without_comment = line.split_once('#').map_or(line, |(before, _)| before);
        without_comment.len() == without_comment.trim_start().len()
            && ["x-dembly:", "'x-dembly':", "\"x-dembly\":"]
                .iter()
                .filter_map(|key| without_comment.trim_end().strip_prefix(key))
                .any(|value| {
                    value.is_empty()
                        || value.chars().next().is_some_and(|character| {
                            character.is_whitespace() || matches!(character, '{' | '[')
                        })
                })
    })
}
