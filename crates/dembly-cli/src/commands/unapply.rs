use dembly_cli::{CliError, HostContext};
use dembly_core::resolve_compose_path;
use dembly_docker::{atomic_replace, ManagedCompose};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let config_path = context.config_path.canonicalize().map_err(|error| {
        CliError::new(format!(
            "cannot resolve config path {}: {error}",
            context.config_path.display()
        ))
    })?;
    let deck_root = config_path
        .parent()
        .ok_or_else(|| CliError::new("config.toml has no parent directory"))?;
    let config =
        dembly_core::load_config(&config_path).map_err(|error| CliError::new(error.to_string()))?;
    let compose_path = resolve_compose_path(deck_root, &config.compose.path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let runtime_path = deck_root.join("runtime");

    let mut compose = ManagedCompose::read(&compose_path).map_err(CliError::new)?;
    compose
        .unapply(&config.compose.service)
        .map_err(|error| CliError::new(error.to_string()))?;
    let compose_bytes = compose.to_bytes().map_err(CliError::new)?;
    validate_runtime_path(deck_root, &runtime_path)?;

    atomic_replace(&compose_path, &compose_bytes).map_err(CliError::new)?;
    match fs::remove_dir_all(&runtime_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliError::new(format!(
                "cannot remove Runtime directory {}: {error}",
                runtime_path.display()
            )))
        }
    }

    writeln!(output, "unapplied: {}", compose_path.display())
        .map_err(|error| CliError::new(format!("cannot write unapply result: {error}")))
}

fn validate_runtime_path(root: &Path, runtime: &Path) -> Result<(), CliError> {
    if runtime.parent() != Some(root)
        || runtime.file_name().and_then(|name| name.to_str()) != Some("runtime")
    {
        return Err(CliError::new(format!(
            "Runtime cleanup target is outside the Deck root: {}",
            runtime.display()
        )));
    }
    match fs::symlink_metadata(runtime) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CliError::new(format!(
            "Runtime cleanup target must not be a symlink: {}",
            runtime.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(CliError::new(format!(
            "Runtime cleanup target is not a directory: {}",
            runtime.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliError::new(format!(
            "cannot inspect Runtime cleanup target {}: {error}",
            runtime.display()
        ))),
    }
}
