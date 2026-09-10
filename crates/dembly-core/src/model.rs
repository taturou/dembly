#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeckDocument {
    pub schema_version: u32,
    pub name: String,
    pub base: Base,
    pub cards: Vec<CardReference>,
    pub environment: Environment,
    pub environment_path_prepend: Vec<String>,
    pub volumes: Vec<Volume>,
    pub binds: Vec<Bind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Base {
    Image { image: String },
    Compose { compose: String, service: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardReference {
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardDocument {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub filesystem: Filesystem,
    pub mount: CardMount,
    pub environment_path_prepend: Vec<String>,
    pub environment: Environment,
    pub exports: Vec<Export>,
    pub volumes: Vec<Volume>,
    pub binds: Vec<Bind>,
    pub post_mount_hooks: Vec<Hook>,
    pub check: Option<CardCheck>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Filesystem {
    pub file_type: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardMount {
    pub target: String,
}

pub type Environment = std::collections::BTreeMap<String, String>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Volume {
    pub name: String,
    pub target: String,
    pub shared: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bind {
    pub source: String,
    pub target: String,
    pub mode: String,
    pub required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Export {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hook {
    pub exec: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardCheck {
    pub exec: String,
    pub args: Vec<String>,
}
