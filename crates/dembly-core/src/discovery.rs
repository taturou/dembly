use crate::CoreError;
use std::path::{Path, PathBuf};

pub fn discover_deck(explicit: Option<&Path>, current_directory: &Path) -> Result<PathBuf, CoreError> {
    let candidate = explicit.map(PathBuf::from).unwrap_or_else(|| current_directory.join("deck.toml"));
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(CoreError::DeckNotFound(candidate))
    }
}
