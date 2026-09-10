use std::process::Command;

use crate::compose_config::{parse_compose_service_config, ComposeServiceConfig};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageConfig {
    pub id: String,
    pub entrypoint: Vec<String>,
    pub command: Vec<String>,
    pub environment: Vec<String>,
    pub user: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbedUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
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

pub fn exec_in_container(
    name: &str,
    user: &str,
    argv: &[String],
) -> Result<std::process::ExitStatus, String> {
    let (program, arguments) = argv.split_first().ok_or("exec command is required")?;
    Command::new("docker")
        .args(["exec", "--user", user, name, program])
        .args(arguments)
        .status()
        .map_err(|error| format!("cannot execute docker exec: {error}"))
}

pub fn probe_image_user(
    image: &str,
    executable: &std::path::Path,
    configured: &str,
) -> Result<ProbedUser, String> {
    let bind = format!("{}:/run/dembly/bin/dembly:ro", executable.display());
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--entrypoint",
            "/run/dembly/bin/dembly",
            "--volume",
            &bind,
            image,
            "__runtime",
            "probe",
            configured,
        ])
        .output()
        .map_err(|error| format!("cannot execute Runtime user probe: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve Runtime user {configured}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value = String::from_utf8(output.stdout)
        .map_err(|_| "Runtime user probe emitted non-UTF-8 output".to_owned())?;
    let fields = value.trim().split('\t').collect::<Vec<_>>();
    if fields.len() != 4 {
        return Err("Runtime user probe emitted malformed output".into());
    }
    Ok(ProbedUser {
        name: fields[0].into(),
        uid: fields[1]
            .parse()
            .map_err(|_| "Runtime user probe emitted invalid UID")?,
        gid: fields[2]
            .parse()
            .map_err(|_| "Runtime user probe emitted invalid GID")?,
        home: fields[3].into(),
    })
}

pub fn compose_service_image(compose: &std::path::Path, service: &str) -> Result<String, String> {
    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            compose.to_string_lossy().as_ref(),
            "config",
            "--images",
            service,
        ])
        .output()
        .map_err(|error| format!("cannot execute docker compose config: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve Compose service {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let image = String::from_utf8(output.stdout)
        .map_err(|_| "docker compose config emitted non-UTF-8 output".to_owned())?
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if image.is_empty() {
        Err(format!("Compose service {service} has no effective image"))
    } else {
        Ok(image)
    }
}

pub fn compose_service_config(
    compose: &std::path::Path,
    service: &str,
) -> Result<ComposeServiceConfig, String> {
    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            compose.to_string_lossy().as_ref(),
            "config",
            "--format",
            "json",
        ])
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

pub fn compose_status(
    arguments: &[std::ffi::OsString],
) -> Result<std::process::ExitStatus, String> {
    Command::new("docker")
        .arg("compose")
        .args(arguments)
        .status()
        .map_err(|error| format!("cannot execute docker compose: {error}"))
}

pub fn compose_service_container(
    arguments: &[std::ffi::OsString],
    service: &str,
) -> Result<String, String> {
    let output = Command::new("docker")
        .arg("compose")
        .args(arguments)
        .args(["ps", "-q", service])
        .output()
        .map_err(|error| format!("cannot execute docker compose ps: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve Compose Runtime service {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let id = String::from_utf8(output.stdout)
        .map_err(|_| "docker compose ps emitted non-UTF-8 output".to_owned())?
        .trim()
        .to_owned();
    if id.is_empty() {
        Err(format!("Compose Runtime service is not running: {service}"))
    } else {
        Ok(id)
    }
}

/// Reports whether a Compose project already has a container for the selected service.
/// `--all` is deliberate: a stopped managed Runtime must not be implicitly replaced.
pub fn compose_service_exists(
    arguments: &[std::ffi::OsString],
    service: &str,
) -> Result<bool, String> {
    let output = Command::new("docker")
        .arg("compose")
        .args(arguments)
        .args(["ps", "--all", "-q", service])
        .output()
        .map_err(|error| format!("cannot execute docker compose ps: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot inspect Compose Runtime service {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(!output.stdout.is_empty() && !output.stdout.iter().all(u8::is_ascii_whitespace))
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
