//! PALANTOUR crate inventory: per-crate file lists, LOC, module trees,
//! and GIS-relevance scoring.

use crate::parse::FileEntry;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct CrateInfo {
    pub name: String,
    pub rs_files: Vec<String>,
    pub other_files: Vec<String>,
    pub total_loc: usize,
    pub has_main: bool,
    pub has_lib: bool,
    pub gis_relevant: bool,
    pub gis_signals: Vec<String>,
}

const CRATE_PREFIX: &str = "PALANTOUR/crates/";

fn crate_of(path: &str) -> Option<String> {
    let rest = path.strip_prefix(CRATE_PREFIX)?;
    let name = rest.split('/').next()?;
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn gis_name_signal(name: &str) -> Option<String> {
    let n = name.to_lowercase();
    for kw in ["geo", "map", "tile", "gis", "odin", "viewer", "raster", "orth", "spatial", "terrain", "survey"] {
        if n.contains(kw) {
            return Some(format!("name:{kw}"));
        }
    }
    None
}

fn gis_content_signals(content: &str) -> Vec<String> {
    let c = content.to_lowercase();
    let mut out = Vec::new();
    for kw in ["mercator", "tile_x", "tile_y", "geojson", "wgs84", "epsg", "orthomosaic", "slippy", "quadkey", "lat_lon", "latitude"] {
        if c.contains(kw) {
            out.push(format!("content:{kw}"));
            if out.len() >= 4 { break; }
        }
    }
    out
}

pub fn build_inventory(entries: &[FileEntry]) -> BTreeMap<String, CrateInfo> {
    let mut map: BTreeMap<String, CrateInfo> = BTreeMap::new();
    for e in entries {
        let Some(cname) = crate_of(&e.path) else { continue };
        let info = map.entry(cname.clone()).or_insert_with(|| CrateInfo {
            name: cname.clone(),
            ..Default::default()
        });
        let loc = e.content.lines().count();
        info.total_loc += loc;
        if e.path.ends_with(".rs") {
            info.rs_files.push(e.path.clone());
            if e.path.ends_with("src/main.rs") { info.has_main = true; }
            if e.path.ends_with("src/lib.rs") { info.has_lib = true; }
            for sig in gis_content_signals(&e.content) {
                if !info.gis_signals.contains(&sig) {
                    info.gis_signals.push(sig);
                }
            }
        } else {
            info.other_files.push(e.path.clone());
        }
        if let Some(sig) = gis_name_signal(&info.name) {
            if !info.gis_signals.contains(&sig) {
                info.gis_signals.push(sig);
            }
        }
    }
    for info in map.values_mut() {
        info.gis_relevant = !info.gis_signals.is_empty();
        info.rs_files.sort();
        info.other_files.sort();
    }
    map
}

pub fn gis_relevant_crates(map: &BTreeMap<String, CrateInfo>) -> Vec<&CrateInfo> {
    map.values().filter(|c| c.gis_relevant).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::FileEntry;

    fn entry(path: &str, content: &str) -> FileEntry {
        FileEntry { path: path.into(), content: content.into() }
    }

    #[test]
    fn inventory_counts_and_flags() {
        let entries = vec![
            entry("PALANTOUR/crates/pt-odin-viewer/src/main.rs", "fn main() { let tile_x = 1; }"),
            entry("PALANTOUR/crates/pt-odin-viewer/Cargo.toml", "[package]"),
            entry("PALANTOUR/crates/pt-coin/src/lib.rs", "fn mint() {}"),
        ];
        let inv = build_inventory(&entries);
        assert_eq!(inv.len(), 2);
        let odin = &inv["pt-odin-viewer"];
        assert!(odin.has_main);
        assert!(odin.gis_relevant);
        assert!(odin.gis_signals.iter().any(|s| s.contains("odin")));
        assert!(!inv["pt-coin"].gis_relevant);
    }
}
