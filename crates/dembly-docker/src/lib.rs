//! Docker and Docker Compose integration.

mod atomic_write;
mod client;
mod compose_config;
mod devcontainer;
mod image;
mod managed_compose;

pub use atomic_write::atomic_replace;

pub use client::{
    compose_service_config, compose_service_container, compose_service_exists,
    compose_service_image, compose_status, container_label, docker_status, docker_status_quiet,
    exec_in_container, image_identity, inspect_image, probe_image_user, run_docker, ImageConfig,
    ProbedUser,
};
pub use compose_config::ComposeServiceConfig;
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
