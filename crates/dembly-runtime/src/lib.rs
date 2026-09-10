//! Runtime initialization.

mod config;
mod mount;

pub use config::{load_runtime_config, RuntimeConfig, RuntimeUser};
pub use mount::{mount_card, squashfs_mount_command, RuntimeCard};
