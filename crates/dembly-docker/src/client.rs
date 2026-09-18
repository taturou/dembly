use crate::compose_config::{parse_compose_service_config, EffectiveService};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageConfig {
    pub id: String,
    pub entrypoint: Vec<String>,
    pub command: Vec<String>,
    pub environment: Vec<String>,
    pub user: String,
}

/// Resolve the selected Compose service without starting or inspecting containers.
pub fn inspect_compose(files: &[PathBuf], service: &str) -> Result<EffectiveService, String> {
    if files.is_empty() {
        return Err("at least one Compose file is required".into());
    }
    let mut command = Command::new("docker");
    command.arg("compose");
    for file in files {
        command.arg("-f").arg(file);
    }
    let output = command
        .args(["config", "--format", "json"])
        .output()
        .map_err(|error| format!("cannot execute docker compose config: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve Compose service {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let json = String::from_utf8(output.stdout)
        .map_err(|_| "docker compose config emitted non-UTF-8 output".to_owned())?;
    parse_compose_service_config(&json, service)
}

/// Inspect image configuration without creating or starting a container.
pub fn inspect_image(reference: &str) -> Result<ImageConfig, String> {
    let output = Command::new("docker")
        .args(["image", "inspect", reference])
        .output()
        .map_err(|error| format!("cannot execute docker image inspect: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot inspect Docker image {reference}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut documents: Vec<ImageInspect> = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid docker image inspect JSON: {error}"))?;
    let document = documents
        .pop()
        .ok_or_else(|| format!("Docker image {reference} was not found"))?;
    if document.id.is_empty() {
        return Err(format!("Docker image {reference} has no immutable ID"));
    }
    Ok(ImageConfig {
        id: document.id,
        entrypoint: document.config.entrypoint.unwrap_or_default(),
        command: document.config.command.unwrap_or_default(),
        environment: document.config.environment.unwrap_or_default(),
        user: document.config.user.unwrap_or_default(),
    })
}

/// Resolve the immutable ID of a locally available Docker image.
pub fn image_identity(reference: &str) -> Result<String, String> {
    inspect_image(reference).map(|image| image.id)
}

#[derive(Deserialize)]
struct ImageInspect {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Config", default)]
    config: ImageInspectConfig,
}

#[derive(Default, Deserialize)]
struct ImageInspectConfig {
    #[serde(rename = "Entrypoint")]
    entrypoint: Option<Vec<String>>,
    #[serde(rename = "Cmd")]
    command: Option<Vec<String>>,
    #[serde(rename = "Env")]
    environment: Option<Vec<String>>,
    #[serde(rename = "User")]
    user: Option<String>,
}
