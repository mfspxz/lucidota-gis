//! saltspring — gis-001: Saltspring Slasher GIS core.
//!
//! Canonical dataset (real named features), layers as GeoJSON, and a static
//! Leaflet site generator. Depends on geo_core only.

pub mod data;
pub mod site;

pub use data::{ferry_routes, layers_geojson, places, region, roads, validate_all};
pub use site::render_site;
