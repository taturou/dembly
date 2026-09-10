//! Dembly domain model and resolution.

mod checksum;
mod discovery;
mod error;
mod lock;
mod model;
mod parse;
mod planning;
mod resolution;

pub use checksum::{sha256_file, verify_card_filesystem};
pub use discovery::discover_deck;
pub use error::CoreError;
pub use lock::{read_lock, write_lock, DeckLock, LockBase, LockedCard};
pub use model::{
    Base, Bind, CardCheck, CardDocument, CardMount, CardReference, DeckDocument, Environment,
    Export, Filesystem, Hook, Volume,
};
pub use parse::{load_card, load_deck};
pub use planning::{
    expand_bind_source, expand_bind_target, plan_environment, resolve_volume_path,
    validate_mount_targets, validate_shared_volume_consistency, BindVariables, CardEnvironment,
    MountResource, VolumeOwner,
};
pub use resolution::{resolve_deck, ResolvedBind, ResolvedCard, ResolvedDeck, ResolvedVolume};
