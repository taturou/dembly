use crate::{CardDocument, ConfigDocument, CoreError};
use std::fs;
use std::path::Path;

pub fn load_config(path: &Path) -> Result<ConfigDocument, CoreError> {
    let document: ConfigDocument = deserialize(path)?;
    if document.schema_version != 1 {
        return Err(CoreError::parse(path, "Config schema_version must be 1"));
    }
    Ok(document)
}

pub fn load_card(path: &Path) -> Result<CardDocument, CoreError> {
    deserialize(path)
}

fn deserialize<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, CoreError> {
    let content = fs::read_to_string(path).map_err(|source| CoreError::Io {
        path: path.into(),
        source,
    })?;
    toml::from_str(&content).map_err(|error| CoreError::parse(path, error.to_string()))
}
