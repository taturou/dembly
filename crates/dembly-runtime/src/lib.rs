//! Runtime initialization.

mod config;
mod mount;
mod setup;
mod user;

pub use config::{
    load_runtime_config, render_runtime_config, RuntimeBind, RuntimeCheck, RuntimeConfig,
    RuntimeExport, RuntimeHook,
};
pub use mount::{
    expand_runtime_bind_target, mount_bind, mount_card, squashfs_mount_command, RuntimeCard,
};
pub use setup::{
    create_exports, ensure_mount_targets_exist, ensure_volume_targets_exist, run_checks, run_hooks,
};
pub use user::{
    resolve_current_runtime_user, resolve_runtime_user, ResolvedRuntimeUser, RuntimeUserSpec,
};
