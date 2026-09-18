use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    let default_name = request
        .tool_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let name = request
        .name
        .as_deref()
        .or((!request.non_interactive).then_some(default_name))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error("--name is required with --non-interactive"))?;
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return Err(error(format!("invalid Card name: {name}")));
    }
    let version = request
        .version
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error("--version is required with --non-interactive"))?;
    let mount_target = request
        .mount_target
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("/opt/dembly/cards/{name}"));
    if !Path::new(&mount_target).is_absolute()
        || mount_target.split('/').any(|segment| segment == "..")
    {
        return Err(error(format!("invalid Card mount target: {mount_target}")));
    }
    if !request.tool_root.is_dir() {
        return Err(error(format!(
            "tool root does not exist: {}",
            request.tool_root.display()
        )));
    }

    let card_root = request.cards_root.join(name);
    fs::create_dir_all(&card_root).map_err(io_error)?;
    let filesystem = card_root.join("rootfs.squashfs");
    let manifest = card_root.join("card.toml");
    let temporary_filesystem = temporary_path(&card_root, "rootfs.squashfs");
    let mut filesystem_guard = TemporaryGuard(Some(temporary_filesystem.clone()));
    let status = Command::new("mksquashfs")
        .arg(&request.tool_root)
        .arg(&temporary_filesystem)
        .args(["-noappend", "-comp", "zstd", "-no-progress"])
        .status()
        .map_err(io_error)?;
    if !status.success() {
        return Err(error(format!("mksquashfs failed with status {status}")));
    }
    File::open(&temporary_filesystem)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    let sha256 = sha256sum(&temporary_filesystem)?;
    let manifest_bytes =
        render_manifest(name, version, &mount_target, &sha256, &request.path_prepend);
    let (temporary_manifest, mut manifest_file) = create_temporary(&card_root, "card.toml")?;
    let mut manifest_guard = TemporaryGuard(Some(temporary_manifest.clone()));
    manifest_file
        .write_all(manifest_bytes.as_bytes())
        .and_then(|()| manifest_file.sync_all())
        .map_err(io_error)?;
    drop(manifest_file);

    commit_card_artifacts(
        &temporary_filesystem,
        &filesystem,
        &temporary_manifest,
        &manifest,
    )?;
    filesystem_guard.0 = None;
    manifest_guard.0 = None;
    Ok(CardBuildResult {
        card_root,
        manifest,
        filesystem,
        sha256,
    })
}

fn commit_card_artifacts(
    staged_filesystem: &Path,
    filesystem: &Path,
    staged_manifest: &Path,
    manifest: &Path,
) -> Result<(), CardBuildError> {
    let filesystem_backup = backup_existing(filesystem)?;
    let manifest_backup = match backup_existing(manifest) {
        Ok(backup) => backup,
        Err(error) => {
            cleanup_backup(filesystem_backup.as_deref());
            return Err(error);
        }
    };

    let commit = fs::rename(staged_filesystem, filesystem)
        .map_err(|source| error(format!("cannot replace {}: {source}", filesystem.display())))
        .and_then(|()| {
            fs::rename(staged_manifest, manifest)
                .map_err(|source| error(format!("cannot replace {}: {source}", manifest.display())))
        })
        .and_then(|()| sync_parent(filesystem));

    if let Err(commit_error) = commit {
        let filesystem_rollback = restore_backup(filesystem, filesystem_backup.as_deref());
        let manifest_rollback = restore_backup(manifest, manifest_backup.as_deref());
        let _ = sync_parent(filesystem);
        cleanup_backup(filesystem_backup.as_deref());
        cleanup_backup(manifest_backup.as_deref());
        if let Err(rollback_error) = filesystem_rollback.and(manifest_rollback) {
            return Err(error(format!(
                "{commit_error}; cannot roll back Card artifacts: {rollback_error}"
            )));
        }
        return Err(commit_error);
    }

    cleanup_backup(filesystem_backup.as_deref());
    cleanup_backup(manifest_backup.as_deref());
    let _ = sync_parent(filesystem);
    Ok(())
}

fn backup_existing(path: &Path) -> Result<Option<PathBuf>, CardBuildError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("artifact");
            let backup = temporary_path(parent, &format!("{name}.backup"));
            fs::hard_link(path, &backup).map_err(io_error)?;
            Ok(Some(backup))
        }
        Ok(_) => Err(error(format!(
            "Card artifact target is not a regular file: {}",
            path.display()
        ))),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(source)),
    }
}

fn restore_backup(path: &Path, backup: Option<&Path>) -> Result<(), CardBuildError> {
    match backup {
        Some(backup) => fs::rename(backup, path).map_err(io_error),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(io_error(source)),
        },
    }
}

fn cleanup_backup(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
}

fn sync_parent(path: &Path) -> Result<(), CardBuildError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

fn create_temporary(parent: &Path, name: &str) -> Result<(PathBuf, File), CardBuildError> {
    for _ in 0..100 {
        let path = temporary_path(parent, name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error(source)),
        }
    }
    Err(error(format!(
        "cannot create temporary Card artifact in {}",
        parent.display()
    )))
}

fn temporary_path(parent: &Path, name: &str) -> PathBuf {
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{name}.dembly-{}-{sequence}.tmp",
        std::process::id()
    ))
}

struct TemporaryGuard(Option<PathBuf>);

impl Drop for TemporaryGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn render_manifest(
    name: &str,
    version: &str,
    mount_target: &str,
    sha256: &str,
    path_prepend: &[String],
) -> String {
    let mut document = format!(
        "schema_version = 1\nname = \"{name}\"\nversion = \"{version}\"\n\n[filesystem]\ntype = \"squashfs\"\nfile = \"rootfs.squashfs\"\nsha256 = \"{sha256}\"\n\n[mount]\ntarget = \"{mount_target}\"\n"
    );
    if !path_prepend.is_empty() {
        let entries = path_prepend
            .iter()
            .map(|entry| format!("\"{entry}\""))
            .collect::<Vec<_>>()
            .join(", ");
        document.push_str(&format!("\n[environment_path]\nprepend = [{entries}]\n"));
    }
    document
}

fn sha256sum(path: &Path) -> Result<String, CardBuildError> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(error(format!(
            "sha256sum failed with status {}",
            output.status
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| error("sha256sum emitted non-UTF-8 output"))?;
    stdout
        .split_whitespace()
        .next()
        .filter(|hash| {
            hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit())
        })
        .map(str::to_owned)
        .ok_or_else(|| error("sha256sum emitted an invalid checksum"))
}

fn io_error(source: std::io::Error) -> CardBuildError {
    error(source.to_string())
}
fn error(message: impl Into<String>) -> CardBuildError {
    CardBuildError(message.into())
}

#[cfg(test)]
mod tests {
    use super::commit_card_artifacts;
    use std::fs;

    #[test]
    fn manifest_commit_failure_restores_existing_filesystem_and_manifest() {
        let root = std::env::temp_dir().join(format!("dembly-card-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let filesystem = root.join("rootfs.squashfs");
        let manifest = root.join("card.toml");
        let staged_filesystem = root.join(".rootfs.squashfs.test.tmp");
        let staged_manifest = root.join(".card.toml.test.tmp");
        fs::write(&filesystem, b"old filesystem").unwrap();
        fs::write(&manifest, b"old manifest").unwrap();
        fs::write(&staged_filesystem, b"new filesystem").unwrap();
        fs::create_dir(&staged_manifest).unwrap();

        let error =
            commit_card_artifacts(&staged_filesystem, &filesystem, &staged_manifest, &manifest)
                .unwrap_err();

        assert!(error.to_string().contains("card.toml"), "{error}");
        assert_eq!(fs::read(&filesystem).unwrap(), b"old filesystem");
        assert_eq!(fs::read(&manifest).unwrap(), b"old manifest");
        fs::remove_dir_all(root).unwrap();
    }
}
