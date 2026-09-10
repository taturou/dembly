//! Dembly domain model and resolution.

mod discovery;
mod error;
mod model;
mod parse;

pub use discovery::discover_deck;
pub use error::CoreError;
pub use model::{Base, CardDocument, CardMount, CardReference, DeckDocument, Filesystem};
pub use parse::{load_card, load_deck};
