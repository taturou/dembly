use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageConfig {
    pub id: String,
    pub entrypoint: Vec<String>,
    pub command: Vec<String>,
    pub environment: Vec<String>,
    pub user: String,
}

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

pub fn inspect_image(reference: &str) -> Result<ImageConfig, String> {
    let id = image_identity(reference)?;
    let entrypoint = inspect_lines(
        reference,
        "{{range .Config.Entrypoint}}{{println .}}{{end}}",
    )?;
    let command = inspect_lines(reference, "{{range .Config.Cmd}}{{println .}}{{end}}")?;
    let environment = inspect_lines(reference, "{{range .Config.Env}}{{println .}}{{end}}")?;
    let user = inspect_lines(reference, "{{.Config.User}}")?
        .into_iter()
        .next()
        .unwrap_or_default();
    Ok(ImageConfig {
        id,
        entrypoint,
        command,
        environment,
        user,
    })
}

pub fn run_docker(arguments: &[std::ffi::OsString]) -> Result<std::process::ExitStatus, String> {
    let (program, args) = arguments.split_first().ok_or("Docker command is empty")?;
    Command::new(program)
        .args(args)
        .status()
        .map_err(|error| format!("cannot execute Docker command: {error}"))
}

pub fn docker_status(arguments: &[&str]) -> Result<std::process::ExitStatus, String> {
    Command::new("docker")
        .args(arguments)
        .status()
        .map_err(|error| format!("cannot execute docker: {error}"))
}

pub fn container_label(name: &str, label: &str) -> Result<String, String> {
    let template = format!("{{{{ index .Config.Labels \"{label}\" }}}}");
    let output = Command::new("docker")
        .args(["container", "inspect", "--format", &template, name])
        .output()
        .map_err(|error| format!("cannot execute docker container inspect: {error}"))?;
    if !output.status.success() {
        return Err(format!("Runtime is not running: {name}"));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "docker container inspect emitted non-UTF-8 output".to_owned())
        .map(|value| value.trim().to_owned())
}

fn inspect_lines(reference: &str, template: &str) -> Result<Vec<String>, String> {
    let output = Command::new("docker")
        .args(["image", "inspect", "--format", template, reference])
        .output()
        .map_err(|error| format!("cannot execute docker image inspect: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot inspect Docker image {reference}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "docker image inspect emitted non-UTF-8 output".to_owned())
        .map(|value| {
            value
                .lines()
                .map(str::to_owned)
                .filter(|value| !value.is_empty())
                .collect()
        })
}
