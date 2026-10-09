//! Slippy-map tile addressing: parent/child traversal, quadkeys, bbox.

use crate::coord::{LatLon, WebMercator};

/// Slippy-map tile address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub z: u8,
}

impl Tile {
    pub fn new(x: u32, y: u32, z: u8) -> Option<Self> {
        if z > 22 {
            return None;
        }
        let m = 2u32.pow(z as u32);
        if x >= m || y >= m {
            return None;
        }
        Some(Self { x, y, z })
    }

    pub fn for_point(p: &LatLon, z: u8) -> Option<Self> {
        let (x, y) = WebMercator::tile_xy(p, z);
        Self::new(x, y, z)
    }

    pub fn parent(&self) -> Option<Self> {
        if self.z == 0 {
            return None;
        }
        Self::new(self.x / 2, self.y / 2, self.z - 1)
    }

    pub fn children(&self) -> Option<[Self; 4]> {
        if self.z >= 22 {
            return None;
        }
        let z = self.z + 1;
        Some([
            Self::new(self.x * 2, self.y * 2, z)?,
            Self::new(self.x * 2 + 1, self.y * 2, z)?,
            Self::new(self.x * 2, self.y * 2 + 1, z)?,
            Self::new(self.x * 2 + 1, self.y * 2 + 1, z)?,
        ])
    }

    /// Bing-style quadkey.
    pub fn quadkey(&self) -> String {
        let mut k = String::with_capacity(self.z as usize);
        for i in (1..=self.z).rev() {
            let mut d = b'0';
            let mask = 1u32 << (i - 1);
            if self.x & mask != 0 {
                d += 1;
            }
            if self.y & mask != 0 {
                d += 2;
            }
            k.push(d as char);
        }
        k
    }

    /// Northwest and southeast corners.
    pub fn bbox(&self) -> (LatLon, LatLon) {
        WebMercator::tile_bbox(self.x, self.y, self.z)
    }

    /// All tiles covering a lat/lon bounding box at zoom z (inclusive).
    pub fn covering(nw: &LatLon, se: &LatLon, z: u8) -> Vec<Tile> {
        let (x0, y0) = WebMercator::tile_xy(nw, z);
        let (x1, y1) = WebMercator::tile_xy(se, z);
        let mut out = Vec::new();
        for x in x0.min(x1)..=x0.max(x1) {
            for y in y0.min(y1)..=y0.max(y1) {
                if let Some(t) = Tile::new(x, y, z) {
                    out.push(t);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadkey_known() {
        let t = Tile::new(3, 5, 3).unwrap();
        assert_eq!(t.quadkey(), "213");
    }

    #[test]
    fn parent_child_inverse() {
        let t = Tile::new(1204, 1532, 12).unwrap();
        let kids = t.children().unwrap();
        for k in &kids {
            assert_eq!(k.parent(), Some(t));
        }
    }

    #[test]
    fn root_has_no_parent() {
        assert_eq!(Tile::new(0, 0, 0).unwrap().parent(), None);
    }

    #[test]
    fn covering_saltspring_z10() {
        // Rough Saltspring Island bbox.
        let nw = LatLon::new_unchecked(48.98, -123.72);
        let se = LatLon::new_unchecked(48.72, -123.28);
        let ts = Tile::covering(&nw, &se, 10);
        assert!(!ts.is_empty() && ts.len() < 60, "tiles={}", ts.len());
        // Every tile bbox must intersect the query box.
        for t in &ts {
            let (tnw, tse) = t.bbox();
            assert!(tnw.lon <= se.lon && tse.lon >= nw.lon);
        }
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(Tile::new(4, 0, 2).is_none()); // x < 2^2
        assert!(Tile::new(0, 0, 23).is_none());
    }
}
