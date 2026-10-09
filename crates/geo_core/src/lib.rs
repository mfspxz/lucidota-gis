//! geo_core: shared GIS primitives for the whole gis domain.
//!
//! Dependency-free by design (crates.io unreachable from this VM): every
//! algorithm is implemented in std-only Rust so builds are deterministic.
//!
//! Modules: coord (LatLon, WebMercator), tile (slippy math), raster (RGB +
//! dependency-free PNG writer + PPM reader), geojson, track (GPS tracks +
//! tile overlay — gis-010's lifelog feed plugs here), validate (spoof
//! rejection — core of gis-014), organelle_shim (gis-008 surface awaiting
//! org-002; deleted when the real organelle host lands).

pub mod coord;
pub mod geojson;
pub mod organelle_shim;
pub mod raster;
pub mod tile;
pub mod track;
pub mod validate;
