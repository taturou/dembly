use serde::{Deserialize, Deserializer};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigDocument {
    pub schema_version: u32,
    pub compose: ComposeReference,
    #[serde(default)]
    pub devcontainer: Option<DevcontainerReference>,
    #[serde(default)]
    pub cards: Vec<CardReference>,
    #[serde(default)]
    pub environment: Environment,
    #[serde(
        default,
        rename = "environment_path",
        deserialize_with = "deserialize_prepend"
    )]
    pub environment_path_prepend: Vec<String>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub binds: Vec<Bind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComposeReference {
    pub path: String,
    pub service: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DevcontainerReference {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardReference {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardDocument {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub filesystem: Filesystem,
    pub mount: CardMount,
    #[serde(
        default,
        rename = "environment_path",
        deserialize_with = "deserialize_prepend"
    )]
    pub environment_path_prepend: Vec<String>,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub exports: Vec<Export>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub binds: Vec<Bind>,
    #[serde(
        default,
        rename = "hooks",
        deserialize_with = "deserialize_post_mount_hooks"
    )]
    pub post_mount_hooks: Vec<Hook>,
    #[serde(default)]
    pub check: Option<CardCheck>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentPath {
    #[serde(default)]
    pub prepend: Vec<String>,
}

fn deserialize_prepend<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    EnvironmentPath::deserialize(deserializer).map(|value| value.prepend)
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    #[serde(default, rename = "post_mount")]
    pub post_mount: Vec<Hook>,
}

fn deserialize_post_mount_hooks<'de, D>(deserializer: D) -> Result<Vec<Hook>, D::Error>
where
    D: Deserializer<'de>,
{
    Hooks::deserialize(deserializer).map(|value| value.post_mount)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Filesystem {
    #[serde(rename = "type")]
    pub file_type: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardMount {
    pub target: String,
}

pub type Environment = std::collections::BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Volume {
    pub name: String,
    pub target: String,
    #[serde(default)]
    pub shared: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bind {
    pub source: String,
    pub target: String,
    pub mode: String,
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardCheck {
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
}
