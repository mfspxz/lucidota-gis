//! palontour: blake2b content-addressed storage + mapping engine.
//!
//! Restores the PALONTOUR role from the repomix pack (64 crates in Zackary's
//! Rust world): the mapping core whose tile/artifact store is a blake2b CAS.
//! Pure std Rust — no external crates.

pub mod blake2b;
pub mod cas;
pub mod tileset;

pub use blake2b::{blake2b_256, blake2b_512, hex};
pub use cas::ContentStore;
pub use tileset::{TileEntry, Tileset};
