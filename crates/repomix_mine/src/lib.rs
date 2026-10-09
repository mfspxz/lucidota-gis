//! repomix_mine — gis-006: mine the 32MB / 3,834-file repomix pack,
//! restore the PALANTOUR crate inventory, extract GIS-relevant modules,
//! and seal the inventory into the palontour blake2b CAS.

pub mod inventory;
pub mod parse;
pub mod restore;

pub use restore::{restore, RestoreReport, PACK_PATH};
