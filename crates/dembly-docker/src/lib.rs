//! Docker and Docker Compose integration.

mod client;
mod compose;
mod image;

pub use client::{
    compose_service_container, compose_service_exists, compose_service_image, compose_status,
    container_label, docker_status, exec_in_container, image_identity, inspect_image,
    probe_image_user, run_docker, ImageConfig, ProbedUser,
};
pub use compose::{compose_override, ComposeRuntimePlan};
pub use image::{image_create_command, CardFileBind, ImageRuntimePlan};
