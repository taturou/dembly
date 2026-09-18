//! Dembly domain model and resolution.

mod checksum;
mod discovery;
mod error;
mod identity;
mod model;
mod parse;
mod planning;
mod resolution;

pub use checksum::{sha256_file, verify_card_filesystem};
pub use discovery::{discover_config, discover_init_candidates, CardCandidate, InitCandidates};
pub use error::CoreError;
pub use identity::{CardIdentity, LockInput};
pub use model::{
    Bind, CardCheck, CardDocument, CardMount, CardReference, ComposeReference, ConfigDocument,
    DevcontainerReference, Environment, Export, Filesystem, Hook, Volume,
};
pub use parse::{load_card, load_config};
pub use planning::{
    expand_bind_source, expand_bind_target, plan_environment, resolve_volume_path,
    validate_mount_targets, validate_shared_volume_consistency, CardEnvironment, HostVariables,
    MountResource, VolumeOwner,
};
pub use resolution::{resolve_deck, ResolvedBind, ResolvedCard, ResolvedDeck, ResolvedVolume};
