use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardBuildRequest {
    pub tool_root: PathBuf,
    pub cards_root: PathBuf,
    pub name: Option<String>,
    pub version: Option<String>,
    pub mount_target: Option<String>,
    pub path_prepend: Vec<String>,
    pub non_interactive: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardBuildResult {
    pub card_root: PathBuf,
    pub manifest: PathBuf,
    pub filesystem: PathBuf,
    pub sha256: String,
}

#[derive(Debug)]
pub struct CardBuildError(String);

impl Display for CardBuildError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CardBuildError {}

pub fn build_card(request: &CardBuildRequest) -> Result<CardBuildResult, CardBuildError> {
    let default_name = request.tool_root.file_name().and_then(|value| value.to_str()).unwrap_or_default();
    let name = request.name.as_deref().or((!request.non_interactive).then_some(default_name))
        .filter(|value| !value.is_empty()).ok_or_else(|| error("--name is required with --non-interactive"))?;
    if !name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-') {
        return Err(error(format!("invalid Card name: {name}")));
    }
    let version = request.version.as_deref().filter(|value| !value.is_empty())
        .ok_or_else(|| error("--version is required with --non-interactive"))?;
    let mount_target = request.mount_target.as_deref().map(str::to_owned)
        .unwrap_or_else(|| format!("/opt/dembly/cards/{name}"));
    if !Path::new(&mount_target).is_absolute() || mount_target.split('/').any(|segment| segment == "..") {
        return Err(error(format!("invalid Card mount target: {mount_target}")));
    }
    if !request.tool_root.is_dir() {
        return Err(error(format!("tool root does not exist: {}", request.tool_root.display())));
    }

    let card_root = request.cards_root.join(name);
    fs::create_dir_all(&card_root).map_err(io_error)?;
    let temporary = card_root.join(".rootfs.squashfs.tmp");
    let filesystem = card_root.join("rootfs.squashfs");
    let _ = fs::remove_file(&temporary);
    let status = Command::new("mksquashfs")
        .arg(&request.tool_root)
        .arg(&temporary)
        .args(["-noappend", "-comp", "zstd", "-no-progress"])
        .status().map_err(io_error)?;
    if !status.success() {
        let _ = fs::remove_file(&temporary);
        return Err(error(format!("mksquashfs failed with status {status}")));
    }
    let sha256 = sha256sum(&temporary)?;
    fs::rename(&temporary, &filesystem).map_err(io_error)?;
    let manifest = card_root.join("card.toml");
    fs::write(&manifest, render_manifest(name, version, &mount_target, &sha256, &request.path_prepend)).map_err(io_error)?;
    Ok(CardBuildResult { card_root, manifest, filesystem, sha256 })
}

fn render_manifest(name: &str, version: &str, mount_target: &str, sha256: &str, path_prepend: &[String]) -> String {
    let mut document = format!(
        "schema_version = 1\nname = \"{name}\"\nversion = \"{version}\"\n\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"{sha256}\"\n\n[mount]\ntarget = \"{mount_target}\"\n"
    );
    if !path_prepend.is_empty() {
        let entries = path_prepend.iter().map(|entry| format!("\"{entry}\"")).collect::<Vec<_>>().join(", ");
        document.push_str(&format!("\n[environment_path]\nprepend = [{entries}]\n"));
    }
    document
}

fn sha256sum(path: &Path) -> Result<String, CardBuildError> {
    let output = Command::new("sha256sum").arg(path).output().map_err(io_error)?;
    if !output.status.success() { return Err(error(format!("sha256sum failed with status {}", output.status))); }
    let stdout = String::from_utf8(output.stdout).map_err(|_| error("sha256sum emitted non-UTF-8 output"))?;
    stdout.split_whitespace().next().filter(|hash| hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit()))
        .map(str::to_owned).ok_or_else(|| error("sha256sum emitted an invalid checksum"))
}

fn io_error(source: std::io::Error) -> CardBuildError { error(source.to_string()) }
fn error(message: impl Into<String>) -> CardBuildError { CardBuildError(message.into()) }
