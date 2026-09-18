use crate::CoreError;
use std::path::{Path, PathBuf};

pub fn discover_config(
    explicit: Option<&Path>,
    current_directory: &Path,
) -> Result<PathBuf, CoreError> {
    let candidate = explicit
        .map(|path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                current_directory.join(path)
            }
        })
        .unwrap_or_else(|| current_directory.join(".dembly/config.toml"));
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(CoreError::ConfigNotFound(candidate))
    }
}
