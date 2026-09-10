#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeckDocument {
    pub schema_version: u32,
    pub name: String,
    pub base: Base,
    pub cards: Vec<CardReference>,
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
