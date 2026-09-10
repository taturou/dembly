use std::process::Command;

/// Resolve the immutable ID of a locally available Docker image.
/// Pulling is intentionally left to the caller so lock creation is explicit.
pub fn image_identity(reference: &str) -> Result<String, String> {
    let output = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", reference])
        .output()
        .map_err(|error| format!("cannot execute docker image inspect: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve Docker image {reference}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let identity = String::from_utf8(output.stdout)
        .map_err(|_| "docker image inspect emitted non-UTF-8 output".to_owned())?
        .trim()
        .to_owned();
    if identity.is_empty() {
        Err(format!("Docker image {reference} has no immutable ID"))
    } else {
        Ok(identity)
    }
}
