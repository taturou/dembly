use crate::{CliError, HostPlan};
use dembly_core::sha256_bytes;
use dembly_docker::{
    atomic_replace, prepare_atomic_replace, AtomicReplacement, ManagedFields, ManagedMount,
};
use dembly_runtime::{
    render_runtime_config, RuntimeBind, RuntimeCard, RuntimeCheck, RuntimeConfig, RuntimeExport,
    RuntimeHook, RuntimeUserSpec,
};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const LOCK_DIGEST_LABEL: &str = "io.dembly.lock-digest";
pub const IMAGE_REFERENCE_DIGEST_LABEL: &str = "io.dembly.image-reference-digest";
pub const RUNTIME_PLAN_DIGEST_LABEL: &str = "io.dembly.runtime-plan-digest";
pub const RUNTIME_BINARY_DIGEST_LABEL: &str = "io.dembly.runtime-binary-digest";

pub struct ApplyArtifacts {
    runtime_path: PathBuf,
    runtime_bytes: Vec<u8>,
    binary_path: PathBuf,
    binary_bytes: Vec<u8>,
    runtime_digest: String,
    binary_digest: String,
    volume_directories: Vec<PathBuf>,
    gitignore_path: PathBuf,
    gitignore_bytes: Vec<u8>,
    deck_root: PathBuf,
}

pub fn build_runtime_config(plan: &HostPlan, lock_digest: &str) -> RuntimeConfig {
    let mut process_argv = plan
        .effective_service
        .entrypoint
        .clone()
        .unwrap_or_else(|| plan.image.entrypoint.clone());
    process_argv.extend(
        plan.effective_service
            .command
            .clone()
            .unwrap_or_else(|| plan.image.command.clone()),
    );

    let cards = plan
        .deck
        .cards
        .iter()
        .map(|card| RuntimeCard {
            name: card.document.name.clone(),
            image: PathBuf::from(format!("/run/dembly/cards/{}.squashfs", card.document.name)),
            mount_target: PathBuf::from(&card.document.mount.target),
        })
        .collect();
    let binds = plan
        .deck
        .binds
        .iter()
        .enumerate()
        .map(|(index, bind)| RuntimeBind {
            source: PathBuf::from(format!("/run/dembly/binds/{index}")),
            target: bind.target.clone(),
            mode: bind.mode.clone(),
        })
        .collect();
    let exports = plan
        .deck
        .cards
        .iter()
        .flat_map(|card| {
            card.document.exports.iter().map(|export| RuntimeExport {
                source: PathBuf::from(&card.document.mount.target).join(&export.source),
                target: PathBuf::from(&export.target),
            })
        })
        .collect();
    let hooks = plan
        .deck
        .cards
        .iter()
        .flat_map(|card| {
            card.document
                .post_mount_hooks
                .iter()
                .map(|hook| RuntimeHook {
                    card: card.document.name.clone(),
                    exec: PathBuf::from(&card.document.mount.target).join(&hook.exec),
                    args: hook.args.clone(),
                })
        })
        .collect();
    let checks = plan
        .deck
        .cards
        .iter()
        .filter_map(|card| {
            card.document.check.as_ref().map(|check| RuntimeCheck {
                card: card.document.name.clone(),
                exec: PathBuf::from(&card.document.mount.target).join(&check.exec),
                args: check.args.clone(),
            })
        })
        .collect();

    RuntimeConfig {
        schema_version: 1,
        lock_digest: lock_digest.into(),
        runtime_user: RuntimeUserSpec {
            spec: plan.intended_user_spec().into(),
        },
        cards,
        volume_targets: plan
            .deck
            .volumes
            .iter()
            .map(|volume| volume.target.clone())
            .collect(),
        binds,
        exports,
        hooks,
        checks,
        environment: plan.environment.clone(),
        process_argv,
    }
}

pub fn build_managed_fields(
    plan: &HostPlan,
    runtime_path: &Path,
    lock_digest: &str,
    runtime_digest: &str,
    binary_digest: &str,
) -> ManagedFields {
    let service = &plan.deck.document.compose.service;
    let runtime_root = runtime_path.parent().unwrap_or(&plan.deck.root);
    let mut mounts = vec![managed_mount(
        runtime_root.join("bin/dembly"),
        "/run/dembly/bin/dembly",
        "ro",
    )];
    mounts.push(managed_mount(
        runtime_path,
        &format!("/run/dembly/runtime/{service}.toml"),
        "ro",
    ));
    mounts.extend(plan.deck.cards.iter().map(|card| {
        let source = card
            .manifest_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(&card.document.filesystem.file);
        managed_mount(
            source,
            &format!("/run/dembly/cards/{}.squashfs", card.document.name),
            "ro",
        )
    }));
    mounts.extend(
        plan.deck
            .volumes
            .iter()
            .map(|volume| managed_mount(&volume.source, &volume.target.to_string_lossy(), "rw")),
    );
    mounts.extend(plan.deck.binds.iter().enumerate().map(|(index, bind)| {
        managed_mount(
            &bind.source,
            &format!("/run/dembly/binds/{index}"),
            &bind.mode,
        )
    }));

    ManagedFields {
        entrypoint: Value::Sequence(vec![
            string("/run/dembly/bin/dembly"),
            string("__runtime"),
            string("init"),
            string(format!("/run/dembly/runtime/{service}.toml")),
        ]),
        command: Value::Sequence(Vec::new()),
        user: string("root"),
        privileged: Value::Bool(true),
        labels: BTreeMap::from([
            (LOCK_DIGEST_LABEL.into(), string(lock_digest)),
            (
                IMAGE_REFERENCE_DIGEST_LABEL.into(),
                string(artifact_digest(plan.static_image.as_bytes())),
            ),
            (RUNTIME_PLAN_DIGEST_LABEL.into(), string(runtime_digest)),
            (RUNTIME_BINARY_DIGEST_LABEL.into(), string(binary_digest)),
        ]),
        mounts,
    }
}

impl ApplyArtifacts {
    pub fn prepare(
        plan: &HostPlan,
        lock_digest: &str,
        executable: &Path,
    ) -> Result<Self, CliError> {
        let runtime_root = plan.deck.root.join("runtime");
        let runtime_path =
            runtime_root.join(format!("{}.toml", plan.deck.document.compose.service));
        let runtime = build_runtime_config(plan, lock_digest);
        let runtime_bytes = render_runtime_config(&runtime)
            .map_err(CliError::new)?
            .into_bytes();
        let binary_bytes = fs::read(executable).map_err(|error| {
            CliError::new(format!(
                "cannot read current executable {}: {error}",
                executable.display()
            ))
        })?;
        let runtime_digest = artifact_digest(&runtime_bytes);
        let binary_digest = artifact_digest(&binary_bytes);
        let gitignore_path = plan
            .deck
            .root
            .parent()
            .ok_or_else(|| CliError::new("Deck root has no project parent"))?
            .join(".gitignore");
        let current_ignore = match fs::read(&gitignore_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(CliError::new(format!(
                    "cannot read {}: {error}",
                    gitignore_path.display()
                )))
            }
        };
        let gitignore_bytes = render_gitignore(&current_ignore)?;
        let volume_directories = plan
            .deck
            .volumes
            .iter()
            .map(|volume| volume.source.clone())
            .collect::<Vec<_>>();

        validate_directory_path(&plan.deck.root, &runtime_root)?;
        validate_directory_path(&plan.deck.root, &runtime_root.join("bin"))?;
        for directory in &volume_directories {
            validate_directory_path(&plan.deck.root, directory)?;
        }

        Ok(Self {
            runtime_path,
            runtime_bytes,
            binary_path: runtime_root.join("bin/dembly"),
            binary_bytes,
            runtime_digest,
            binary_digest,
            volume_directories,
            gitignore_path,
            gitignore_bytes,
            deck_root: plan.deck.root.clone(),
        })
    }

    pub fn runtime_path(&self) -> &Path {
        &self.runtime_path
    }

    pub fn runtime_digest(&self) -> &str {
        &self.runtime_digest
    }

    pub fn binary_digest(&self) -> &str {
        &self.binary_digest
    }

    pub fn write(self, compose_path: &Path, compose_bytes: &[u8]) -> Result<(), CliError> {
        let runtime_root = self
            .runtime_path
            .parent()
            .expect("runtime plan always has a parent");
        let mut created_directories = Vec::new();
        if let Err(error) =
            create_directory_path(&self.deck_root, runtime_root, &mut created_directories)
        {
            remove_created_directories(&created_directories);
            return Err(error);
        }
        if let Err(error) = create_directory_path(
            &self.deck_root,
            self.binary_path
                .parent()
                .expect("Runtime binary always has a parent"),
            &mut created_directories,
        ) {
            remove_created_directories(&created_directories);
            return Err(error);
        }
        let mut writer = NativeApplyWriter {
            deck_root: &self.deck_root,
            staged: Vec::new(),
            snapshots: BTreeMap::new(),
            created_directories,
            finished: false,
        };
        write_apply_targets(&self, compose_path, compose_bytes, &mut writer)
    }
}

pub fn artifact_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_bytes(bytes))
}

trait ApplyWriter {
    fn replace(&mut self, path: &Path, bytes: &[u8]) -> Result<(), CliError>;
    fn make_executable(&mut self, path: &Path) -> Result<(), CliError>;
    fn create_volume_directory(&mut self, path: &Path) -> Result<(), CliError>;
    fn finish(&mut self) -> Result<(), CliError>;
}

struct NativeApplyWriter<'a> {
    deck_root: &'a Path,
    staged: Vec<AtomicReplacement>,
    snapshots: BTreeMap<PathBuf, FileSnapshot>,
    created_directories: Vec<PathBuf>,
    finished: bool,
}

enum FileSnapshot {
    Missing,
    File {
        bytes: Vec<u8>,
        permissions: fs::Permissions,
    },
    Other,
}

impl ApplyWriter for NativeApplyWriter<'_> {
    fn replace(&mut self, path: &Path, bytes: &[u8]) -> Result<(), CliError> {
        if !self.snapshots.contains_key(path) {
            self.snapshots.insert(path.into(), snapshot_file(path)?);
        }
        let replacement = prepare_atomic_replace(path, bytes).map_err(CliError::new)?;
        self.staged.push(replacement);
        Ok(())
    }

    fn make_executable(&mut self, path: &Path) -> Result<(), CliError> {
        let replacement = self
            .staged
            .iter()
            .rev()
            .find(|replacement| replacement.target() == path)
            .ok_or_else(|| {
                CliError::new(format!(
                    "Runtime binary was not staged before permission update: {}",
                    path.display()
                ))
            })?;
        replacement
            .set_permissions(fs::Permissions::from_mode(0o755))
            .map_err(CliError::new)
    }

    fn create_volume_directory(&mut self, path: &Path) -> Result<(), CliError> {
        create_directory_path(self.deck_root, path, &mut self.created_directories)
    }

    fn finish(&mut self) -> Result<(), CliError> {
        let mut committed = Vec::new();
        while !self.staged.is_empty() {
            let replacement = self.staged.remove(0);
            let target = replacement.target().to_path_buf();
            let result = replacement.commit().map_err(CliError::new);
            committed.push(target);
            if let Err(commit_error) = result {
                let rollback = rollback_files(&committed, &self.snapshots);
                remove_created_directories(&self.created_directories);
                self.finished = true;
                return match rollback {
                    Ok(()) => Err(commit_error),
                    Err(rollback_error) => Err(CliError::new(format!(
                        "{commit_error}; apply rollback failed: {rollback_error}"
                    ))),
                };
            }
        }
        self.finished = true;
        Ok(())
    }
}

impl Drop for NativeApplyWriter<'_> {
    fn drop(&mut self) {
        if !self.finished {
            remove_created_directories(&self.created_directories);
        }
    }
}

fn write_apply_targets(
    artifacts: &ApplyArtifacts,
    compose_path: &Path,
    compose_bytes: &[u8],
    writer: &mut impl ApplyWriter,
) -> Result<(), CliError> {
    writer.replace(&artifacts.runtime_path, &artifacts.runtime_bytes)?;
    writer.replace(&artifacts.binary_path, &artifacts.binary_bytes)?;
    writer.make_executable(&artifacts.binary_path)?;
    for directory in &artifacts.volume_directories {
        writer.create_volume_directory(directory)?;
    }
    writer.replace(&artifacts.gitignore_path, &artifacts.gitignore_bytes)?;
    writer.replace(compose_path, compose_bytes)?;
    writer.finish()
}

fn snapshot_file(path: &Path) -> Result<FileSnapshot, CliError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(FileSnapshot::File {
            bytes: fs::read(path).map_err(|error| {
                CliError::new(format!("cannot snapshot {}: {error}", path.display()))
            })?,
            permissions: metadata.permissions(),
        }),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CliError::new(format!(
            "apply target must not be a symlink: {}",
            path.display()
        ))),
        Ok(_) => Ok(FileSnapshot::Other),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileSnapshot::Missing),
        Err(error) => Err(CliError::new(format!(
            "cannot inspect apply target {}: {error}",
            path.display()
        ))),
    }
}

fn rollback_files(
    committed: &[PathBuf],
    snapshots: &BTreeMap<PathBuf, FileSnapshot>,
) -> Result<(), CliError> {
    for path in committed.iter().rev() {
        match snapshots
            .get(path)
            .expect("every staged apply target has a snapshot")
        {
            FileSnapshot::Missing => match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(CliError::new(format!(
                        "cannot remove new apply target {}: {error}",
                        path.display()
                    )))
                }
            },
            FileSnapshot::File { bytes, permissions } => {
                atomic_replace(path, bytes).map_err(CliError::new)?;
                fs::set_permissions(path, permissions.clone()).map_err(|error| {
                    CliError::new(format!(
                        "cannot restore permissions on {}: {error}",
                        path.display()
                    ))
                })?;
            }
            FileSnapshot::Other => {}
        }
    }
    Ok(())
}

fn managed_mount(source: impl AsRef<Path>, target: &str, mode: &str) -> ManagedMount {
    ManagedMount {
        target: target.into(),
        value: string(format!("{}:{target}:{mode}", source.as_ref().display())),
    }
}

fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}

fn render_gitignore(current: &[u8]) -> Result<Vec<u8>, CliError> {
    let source =
        std::str::from_utf8(current).map_err(|_| CliError::new(".gitignore must be UTF-8"))?;
    let required = ["/.dembly/runtime/", "/.dembly/volumes/"];
    let present = source.lines().collect::<std::collections::BTreeSet<_>>();
    let missing = required
        .into_iter()
        .filter(|rule| !present.contains(rule))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(current.to_vec());
    }

    let mut output = current.to_vec();
    if !output.is_empty() && !output.ends_with(b"\n") {
        output.push(b'\n');
    }
    for rule in missing {
        output.extend_from_slice(rule.as_bytes());
        output.push(b'\n');
    }
    Ok(output)
}

fn validate_directory_path(root: &Path, target: &Path) -> Result<(), CliError> {
    let relative = target.strip_prefix(root).map_err(|_| {
        CliError::new(format!(
            "artifact directory escapes Deck root: {}",
            target.display()
        ))
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(CliError::new(format!(
                    "artifact directory must not traverse a symlink: {}",
                    current.display()
                )))
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(CliError::new(format!(
                    "artifact directory path is not a directory: {}",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CliError::new(format!(
                    "cannot inspect artifact directory {}: {error}",
                    current.display()
                )))
            }
        }
    }
    Ok(())
}

fn create_directory_path(
    root: &Path,
    target: &Path,
    created_directories: &mut Vec<PathBuf>,
) -> Result<(), CliError> {
    validate_directory_path(root, target)?;
    let relative = target.strip_prefix(root).expect("path validated above");
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => created_directories.push(current.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&current).map_err(|error| {
                    CliError::new(format!("cannot inspect {}: {error}", current.display()))
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(CliError::new(format!(
                        "artifact directory is unsafe: {}",
                        current.display()
                    )));
                }
            }
            Err(error) => {
                return Err(CliError::new(format!(
                    "cannot create artifact directory {}: {error}",
                    current.display()
                )))
            }
        }
    }
    Ok(())
}

fn remove_created_directories(directories: &[PathBuf]) {
    for directory in directories.iter().rev() {
        let _ = fs::remove_dir(directory);
    }
}

#[cfg(test)]
mod tests {
    use super::{write_apply_targets, ApplyArtifacts, ApplyWriter};
    use crate::CliError;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    #[derive(Debug, Eq, PartialEq)]
    enum WriteEvent {
        Replace(PathBuf),
        Executable(PathBuf),
        Volume(PathBuf),
    }

    #[derive(Default)]
    struct RecordingWriter {
        events: Vec<WriteEvent>,
    }

    impl ApplyWriter for RecordingWriter {
        fn replace(&mut self, path: &Path, _bytes: &[u8]) -> Result<(), CliError> {
            self.events.push(WriteEvent::Replace(path.into()));
            Ok(())
        }

        fn make_executable(&mut self, path: &Path) -> Result<(), CliError> {
            self.events.push(WriteEvent::Executable(path.into()));
            Ok(())
        }

        fn create_volume_directory(&mut self, path: &Path) -> Result<(), CliError> {
            self.events.push(WriteEvent::Volume(path.into()));
            Ok(())
        }

        fn finish(&mut self) -> Result<(), CliError> {
            Ok(())
        }
    }

    #[test]
    fn apply_replaces_artifacts_in_runtime_binary_volume_ignore_compose_order() {
        let artifacts = ApplyArtifacts {
            runtime_path: PathBuf::from("/deck/.dembly/runtime/dev.toml"),
            runtime_bytes: b"runtime".to_vec(),
            binary_path: PathBuf::from("/deck/.dembly/runtime/bin/dembly"),
            binary_bytes: b"binary".to_vec(),
            runtime_digest: "sha256:runtime".into(),
            binary_digest: "sha256:binary".into(),
            volume_directories: vec![
                PathBuf::from("/deck/.dembly/volumes/cache"),
                PathBuf::from("/deck/.dembly/volumes/tool/private"),
            ],
            gitignore_path: PathBuf::from("/deck/.gitignore"),
            gitignore_bytes: b"ignore".to_vec(),
            deck_root: PathBuf::from("/deck/.dembly"),
        };
        let mut writer = RecordingWriter::default();

        write_apply_targets(
            &artifacts,
            Path::new("/deck/compose.yaml"),
            b"compose",
            &mut writer,
        )
        .unwrap();

        assert_eq!(
            writer.events,
            vec![
                WriteEvent::Replace(PathBuf::from("/deck/.dembly/runtime/dev.toml")),
                WriteEvent::Replace(PathBuf::from("/deck/.dembly/runtime/bin/dembly")),
                WriteEvent::Executable(PathBuf::from("/deck/.dembly/runtime/bin/dembly")),
                WriteEvent::Volume(PathBuf::from("/deck/.dembly/volumes/cache")),
                WriteEvent::Volume(PathBuf::from("/deck/.dembly/volumes/tool/private")),
                WriteEvent::Replace(PathBuf::from("/deck/.gitignore")),
                WriteEvent::Replace(PathBuf::from("/deck/compose.yaml")),
            ]
        );
    }

    #[test]
    fn late_compose_failure_restores_every_artifact_and_new_volume_directory() {
        let root =
            std::env::temp_dir().join(format!("dembly-apply-transaction-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let deck_root = root.join(".dembly");
        let runtime_path = deck_root.join("runtime/dev.toml");
        let binary_path = deck_root.join("runtime/bin/dembly");
        let gitignore_path = root.join(".gitignore");
        let existing_volume = deck_root.join("volumes/existing");
        let new_volume = deck_root.join("volumes/new");
        let compose_path = root.join("compose.yaml");
        fs::create_dir_all(binary_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&existing_volume).unwrap();
        fs::create_dir_all(&compose_path).unwrap();
        fs::write(compose_path.join("old-compose-marker"), b"old compose").unwrap();
        fs::write(&runtime_path, b"old runtime").unwrap();
        fs::write(&binary_path, b"old binary").unwrap();
        let mut permissions = fs::metadata(&binary_path).unwrap().permissions();
        permissions.set_mode(0o744);
        fs::set_permissions(&binary_path, permissions).unwrap();
        fs::write(&gitignore_path, b"old ignore").unwrap();
        fs::write(existing_volume.join("marker"), b"persistent").unwrap();
        let artifacts = ApplyArtifacts {
            runtime_path: runtime_path.clone(),
            runtime_bytes: b"new runtime".to_vec(),
            binary_path: binary_path.clone(),
            binary_bytes: b"new binary".to_vec(),
            runtime_digest: "sha256:new-runtime".into(),
            binary_digest: "sha256:new-binary".into(),
            volume_directories: vec![existing_volume.clone(), new_volume.clone()],
            gitignore_path: gitignore_path.clone(),
            gitignore_bytes: b"new ignore".to_vec(),
            deck_root,
        };

        let error = artifacts.write(&compose_path, b"new compose").unwrap_err();

        assert!(error.to_string().contains("compose.yaml"), "{error}");
        assert_eq!(fs::read(&runtime_path).unwrap(), b"old runtime");
        assert_eq!(fs::read(&binary_path).unwrap(), b"old binary");
        assert_eq!(
            fs::metadata(&binary_path).unwrap().permissions().mode() & 0o777,
            0o744
        );
        assert_eq!(fs::read(&gitignore_path).unwrap(), b"old ignore");
        assert_eq!(
            fs::read(existing_volume.join("marker")).unwrap(),
            b"persistent"
        );
        assert!(!new_volume.exists());
        assert_eq!(
            fs::read(compose_path.join("old-compose-marker")).unwrap(),
            b"old compose"
        );
    }
}
