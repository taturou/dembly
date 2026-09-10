//! Docker and Docker Compose integration.

mod client;
mod compose;
mod image;

pub use client::image_identity;
pub use compose::{compose_override, ComposeRuntimePlan};
pub use image::{image_create_command, CardFileBind, ImageRuntimePlan};
