//! Docker and Docker Compose integration.

mod atomic_write;
mod client;
mod compose_config;
mod devcontainer;
mod image;
mod managed_compose;

pub use atomic_write::atomic_replace;

pub use client::{image_identity, inspect_compose, inspect_image, ImageConfig};
pub use compose_config::EffectiveService;
pub use devcontainer::{
    load_devcontainer, validate_devcontainer, DevContainerCommand, DevContainerCommandArguments,
    DevContainerDocument,
};
pub use image::{image_create_command, CardFileBind, ImageRuntimePlan};
pub use managed_compose::{
    AppliedFields, ApplyState, CardLock, Conflict, DemblyLock, FieldState, LabelState,
    ManagedCompose, ManagedFields, ManagedMount, ManagedMountState, ManagedValue, MountState,
    ServiceSnapshot,
};
