use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("replacement path has no file name: {}", path.display()))?;
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "cannot inspect replacement target {}: {error}",
                path.display()
            ))
        }
    };

    let (temporary_path, mut temporary) = create_temporary(parent, file_name)?;
    let mut guard = TemporaryGuard(Some(temporary_path.clone()));
    if let Some(permissions) = permissions {
        temporary.set_permissions(permissions).map_err(|error| {
            format!(
                "cannot copy permissions to temporary file {}: {error}",
                temporary_path.display()
            )
        })?;
    }
    temporary.write_all(bytes).map_err(|error| {
        format!(
            "cannot write temporary file {}: {error}",
            temporary_path.display()
        )
    })?;
    temporary.sync_all().map_err(|error| {
        format!(
            "cannot sync temporary file {}: {error}",
            temporary_path.display()
        )
    })?;
    drop(temporary);
    fs::rename(&temporary_path, path).map_err(|error| {
        format!(
            "cannot replace {} with {}: {error}",
            path.display(),
            temporary_path.display()
        )
    })?;
    guard.0 = None;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("cannot sync parent directory {}: {error}", parent.display()))
}

fn create_temporary(parent: &Path, file_name: &std::ffi::OsStr) -> Result<(PathBuf, File), String> {
    for _ in 0..100 {
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_name = format!(
            ".{}.dembly-{}-{sequence}.tmp",
            file_name.to_string_lossy(),
            std::process::id()
        );
        let temporary_path = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "cannot create temporary file in {}: {error}",
                    parent.display()
                ))
            }
        }
    }
    Err(format!(
        "cannot create unique temporary file in {}",
        parent.display()
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
