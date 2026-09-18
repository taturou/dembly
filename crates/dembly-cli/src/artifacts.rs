use crate::{CliError, HostPlan};
use dembly_docker::{atomic_replace, ManagedFields, ManagedMount};
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

pub struct ApplyArtifacts {
    runtime_path: PathBuf,
    runtime_bytes: Vec<u8>,
    binary_path: PathBuf,
    binary_bytes: Vec<u8>,
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
        labels: BTreeMap::from([(LOCK_DIGEST_LABEL.into(), string(lock_digest))]),
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
            volume_directories,
            gitignore_path,
            gitignore_bytes,
            deck_root: plan.deck.root.clone(),
        })
    }

    pub fn runtime_path(&self) -> &Path {
        &self.runtime_path
    }

    pub fn write(self) -> Result<(), CliError> {
        let runtime_root = self
            .runtime_path
            .parent()
            .expect("runtime plan always has a parent");
        create_directory_path(&self.deck_root, runtime_root)?;
        create_directory_path(
            &self.deck_root,
            self.binary_path
                .parent()
                .expect("Runtime binary always has a parent"),
        )?;
        atomic_replace(&self.runtime_path, &self.runtime_bytes).map_err(CliError::new)?;
        atomic_replace(&self.binary_path, &self.binary_bytes).map_err(CliError::new)?;
        let mut permissions = fs::metadata(&self.binary_path)
            .map_err(|error| {
                CliError::new(format!(
                    "cannot inspect Runtime binary {}: {error}",
                    self.binary_path.display()
                ))
            })?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&self.binary_path, permissions).map_err(|error| {
            CliError::new(format!(
                "cannot make Runtime binary {} executable: {error}",
                self.binary_path.display()
            ))
        })?;
        for directory in &self.volume_directories {
            create_directory_path(&self.deck_root, directory)?;
        }
        atomic_replace(&self.gitignore_path, &self.gitignore_bytes).map_err(CliError::new)
    }
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
    let mut lines = source
        .lines()
        .filter(|line| !required.contains(line))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.extend(required.into_iter().map(str::to_owned));
    let mut output = lines.join("\n").into_bytes();
    output.push(b'\n');
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

fn create_directory_path(root: &Path, target: &Path) -> Result<(), CliError> {
    validate_directory_path(root, target)?;
    let relative = target.strip_prefix(root).expect("path validated above");
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => {}
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
