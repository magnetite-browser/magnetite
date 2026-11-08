//! Engine-independent browser domain model: profiles, spaces, tabs, folders.
//!
//! This crate must never depend on CEF or any UI toolkit so the logic stays
//! unit-testable and the engine stays swappable. Everything here is plain
//! data plus the operations the sidebar needs (`sidebar` module).

pub mod links;
pub mod model;
pub mod sidebar;
pub mod smart;
pub mod split;

pub use model::*;
