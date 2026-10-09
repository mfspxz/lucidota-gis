//! Mapping engine: a Tileset is a content-addressed index of (z,x,y)->digest.
//! The root digest commits to the whole set — any tile change changes the root.

use crate::blake2b::{blake2b_256, hex};
use crate::cas::ContentStore;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TileEntry {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub digest: String,
}

#[derive(Debug, Default)]
pub struct Tileset {
    pub name: String,
    entries: BTreeMap<(u8, u32, u32), String>,
}

impl Tileset {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, z: u8, x: u32, y: u32, digest: String) {
        self.entries.insert((z, x, y), digest);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, z: u8, x: u32, y: u32) -> Option<&String> {
        self.entries.get(&(z, x, y))
    }

    /// Root digest: blake2b-256 over sorted "z/x/y:digest\n" lines.
    pub fn root_digest(&self) -> String {
        let mut buf = Vec::new();
        for ((z, x, y), d) in &self.entries {
            buf.extend_from_slice(format!("{z}/{x}/{y}:{d}\n").as_bytes());
        }
        hex(&blake2b_256(&buf))
    }

    pub fn to_json(&self) -> String {
        let mut parts = Vec::new();
        for ((z, x, y), d) in &self.entries {
            parts.push(format!(
                "{{\"z\":{z},\"x\":{x},\"y\":{y},\"digest\":\"{d}\"}}"
            ));
        }
        format!(
            "{{\"name\":\"{}\",\"root\":\"{}\",\"tiles\":[{}]}}",
            crate_escape(&self.name),
            self.root_digest(),
            parts.join(",")
        )
    }

    /// Store the index JSON itself in the CAS; returns its digest.
    pub fn seal(&self, store: &ContentStore) -> std::io::Result<String> {
        store.put(self.to_json().as_bytes())
    }
}

fn crate_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_order_independent() {
        let mut a = Tileset::new("t");
        a.insert(10, 1, 2, "aa".into());
        a.insert(10, 3, 4, "bb".into());
        let mut b = Tileset::new("t");
        b.insert(10, 3, 4, "bb".into());
        b.insert(10, 1, 2, "aa".into());
        assert_eq!(a.root_digest(), b.root_digest());
    }

    #[test]
    fn root_changes_on_tile_change() {
        let mut a = Tileset::new("t");
        a.insert(10, 1, 2, "aa".into());
        let mut b = Tileset::new("t");
        b.insert(10, 1, 2, "cc".into());
        assert_ne!(a.root_digest(), b.root_digest());
    }

    #[test]
    fn seal_roundtrip() {
        let p = std::env::temp_dir().join("palontour_seal");
        let _ = std::fs::remove_dir_all(&p);
        let s = ContentStore::open(&p).unwrap();
        let mut t = Tileset::new("saltspring-z10");
        t.insert(10, 301, 372, "deadbeef".into());
        let d = t.seal(&s).unwrap();
        let back = String::from_utf8(s.get(&d).unwrap()).unwrap();
        assert!(back.contains("\"root\""));
        assert!(back.contains("saltspring-z10"));
    }
}
