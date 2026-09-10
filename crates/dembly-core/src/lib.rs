//! Dembly domain model and resolution.

mod checksum;
mod discovery;
mod error;
mod model;
mod parse;
mod planning;

pub use discovery::discover_deck;
pub use checksum::verify_card_filesystem;
pub use error::CoreError;
pub use model::{Base, CardDocument, CardMount, CardReference, DeckDocument, Filesystem};
pub use parse::{load_card, load_deck};
pub use planning::{
    expand_bind_source, expand_bind_target, plan_environment, resolve_volume_path,
    validate_mount_targets, validate_shared_volume_consistency, BindVariables, CardEnvironment,
    MountResource, VolumeOwner,
};
