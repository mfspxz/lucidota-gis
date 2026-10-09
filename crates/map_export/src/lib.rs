//! map_export — gis-013: map print/export.
//!
//! SVG renderer (screen/print vector) + real PDF writer (raster embedded via
//! FlateDecode, xref-validated). Page presets: A4 / Letter, portrait/landscape.

pub mod pdf;
pub mod svg;

pub use pdf::{raster_to_pdf, validate_xref, PageSize};
pub use svg::{SvgGeom, SvgMap};
