//! offline_packs — gis-009: downloadable offline map packs.
//!
//! Format `.gispack`: a tar containing manifest.json (blake2b-256 per tile),
//! tiles/{z}/{x}/{y}.png, layers/*.geojson. readable by system tar.
//!
//! INTEGRITY, NOT TAMPER-EVIDENCE (R-2/R-3/R-8): the manifest digests detect
//! accidental corruption and tampering by a party who can modify tile bytes
//! but not the manifest. The manifest itself is UNSIGNED — a writer who can
//! rewrite the whole pack can rewrite the manifest too, silently. Pack
//! authenticity (who published it) is UNKNOWN; only byte-integrity against
//! the manifest is checked, and only on the read path (`verify` recomputes
//! every tile digest). Do not call packs tamper-evident.

pub mod tar;

use palontour::{blake2b_256, hex};
use std::path::Path;
use tar::{read_tar, write_tar};

#[derive(Debug, Clone)]
pub struct PackTile {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub png: Vec<u8>,
    pub digest: String,
}

pub struct PackBuilder {
    pub name: String,
    pub region_json: String,
    tiles: Vec<PackTile>,
    layers: Vec<(String, String)>,
}

impl PackBuilder {
    pub fn new(name: &str, region_json: &str) -> Self {
        Self { name: name.into(), region_json: region_json.into(), tiles: Vec::new(), layers: Vec::new() }
    }

    pub fn add_tile(&mut self, z: u8, x: u32, y: u32, png: Vec<u8>) {
        let digest = hex(&blake2b_256(&png));
        self.tiles.push(PackTile { z, x, y, png, digest });
    }

    pub fn add_layer(&mut self, name: &str, geojson: &str) {
        self.layers.push((name.into(), geojson.into()));
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Build the .gispack file. Returns the manifest digest (the pack's identity).
    pub fn build(self, path: &str) -> std::io::Result<String> {
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        let mut tile_json = Vec::new();
        for t in &self.tiles {
            files.push((format!("tiles/{}/{}/{}.png", t.z, t.x, t.y), t.png.clone()));
            tile_json.push(format!(
                "{{\"z\":{},\"x\":{},\"y\":{},\"digest\":\"{}\",\"size\":{}}}",
                t.z, t.x, t.y, t.digest, t.png.len()
            ));
        }
        let layer_names: Vec<String> = self.layers.iter().map(|(n, _)| format!("\"{n}\"")).collect();
        for (n, gj) in &self.layers {
            files.push((format!("layers/{n}.geojson"), gj.as_bytes().to_vec()));
        }
        let manifest = format!(
            "{{\"name\":\"{}\",\"format\":\"gispack-1\",\"region\":{},\"tiles\":[{}],\"layers\":[{}]}}",
            self.name, self.region_json, tile_json.join(","), layer_names.join(",")
        );
        let manifest_digest = hex(&blake2b_256(manifest.as_bytes()));
        files.push(("manifest.json".into(), manifest.as_bytes().to_vec()));
        let tar_bytes = write_tar(&files);
        std::fs::write(path, &tar_bytes)?;
        let _ = Path::new(path); // keep Path import meaningful for future validation
        Ok(manifest_digest)
    }
}

#[derive(Debug)]
pub struct PackInfo {
    pub name: String,
    pub tiles: Vec<PackTile>,
    pub layers: Vec<String>,
}

pub struct PackReader;

impl PackReader {
    pub fn open(path: &str) -> Result<PackInfo, String> {
        let buf = std::fs::read(path).map_err(|e| e.to_string())?;
        let entries = read_tar(&buf)?;
        let manifest = entries.iter().find(|e| e.name == "manifest.json").ok_or("no manifest.json")?;
        let ms = String::from_utf8_lossy(&manifest.data);
        let name = ms.split("\"name\":\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("?").to_string();
        // parse tile digests from manifest
        let mut tiles = Vec::new();
        for e in &entries {
            if let Some(rest) = e.name.strip_prefix("tiles/").and_then(|s| s.strip_suffix(".png")) {
                let mut parts = rest.split('/');
                let (z, x, y) = (
                    parts.next().unwrap_or("0").parse().unwrap_or(0),
                    parts.next().unwrap_or("0").parse().unwrap_or(0),
                    parts.next().unwrap_or("0").parse().unwrap_or(0),
                );
                let needle = format!("\"z\":{z},\"x\":{x},\"y\":{y},\"digest\":\"");
                let digest = ms.split(&needle).nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
                tiles.push(PackTile { z, x, y, png: e.data.clone(), digest });
            }
        }
        let layers: Vec<String> = entries.iter()
            .filter_map(|e| e.name.strip_prefix("layers/").and_then(|s| s.strip_suffix(".geojson")))
            .map(|s| s.to_string()).collect();
        Ok(PackInfo { name, tiles, layers })
    }

    /// Verify every tile's bytes against its manifest digest.
    pub fn verify(info: &PackInfo) -> Result<usize, String> {
        for t in &info.tiles {
            if hex(&blake2b_256(&t.png)) != t.digest {
                return Err(format!("tile {}/{}/{} digest mismatch (tampered)", t.z, t.x, t.y));
            }
        }
        Ok(info.tiles.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png() -> Vec<u8> {
        let r = geo_core::raster::Raster::filled(256, 256, 30, 120, 60);
        let mut b = Vec::new();
        r.write_png(&mut b).unwrap();
        b
    }

    #[test]
    fn pack_build_read_verify() {
        let p = "/tmp/offline_pack_test.gispack";
        let mut b = PackBuilder::new("saltspring-z10", "{\"region\":\"saltspring-island\"}");
        b.add_tile(10, 301, 372, sample_png());
        b.add_tile(10, 302, 372, sample_png());
        b.add_layer("places", "{\"type\":\"FeatureCollection\",\"features\":[]}");
        let digest = b.build(p).unwrap();
        assert_eq!(digest.len(), 64);
        let info = PackReader::open(p).unwrap();
        assert_eq!(info.name, "saltspring-z10");
        assert_eq!(info.tiles.len(), 2);
        assert_eq!(PackReader::verify(&info).unwrap(), 2);
        assert_eq!(info.layers, vec!["places".to_string()]);
    }

    #[test]
    fn verify_detects_tamper() {
        let p = "/tmp/offline_pack_tamper.gispack";
        let mut b = PackBuilder::new("t", "{}");
        b.add_tile(10, 1, 2, sample_png());
        b.build(p).unwrap();
        let mut info = PackReader::open(p).unwrap();
        info.tiles[0].png[100] ^= 0xff;
        assert!(PackReader::verify(&info).is_err());
    }

    #[test]
    fn pack_readable_by_system_tar() {
        let p = "/tmp/offline_pack_tar.gispack";
        let mut b = PackBuilder::new("t", "{}");
        b.add_tile(10, 1, 2, sample_png());
        b.build(p).unwrap();
        let o = std::process::Command::new("tar").args(["tf", p]).output().unwrap();
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stdout).contains("manifest.json"));
    }
}
