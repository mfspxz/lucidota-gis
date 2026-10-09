//! GPS tracks: polylines with timestamps, stats, simplification, raster overlay.
//!
//! gis-010 (lifelog feed from beat-006) plugs in here: any source that can
//! produce `TrackPoint`s feeds the same overlay pipeline. Until beat-006 is
//! available this ships with synthetic + imported GPX-lite tracks.

use crate::coord::{LatLon, WebMercator};
use crate::geojson::{Feature, FeatureCollection, Geometry};
use crate::raster::Raster;

#[derive(Debug, Clone)]
pub struct TrackPoint {
    pub pos: LatLon,
    pub ts_unix: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub points: Vec<TrackPoint>,
    pub label: String,
}

impl Track {
    pub fn new(label: &str) -> Self {
        Self { points: Vec::new(), label: label.to_string() }
    }

    pub fn push(&mut self, p: TrackPoint) {
        self.points.push(p);
    }

    pub fn total_distance_m(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| w[0].pos.distance_m(&w[1].pos))
            .sum()
    }

    pub fn duration_s(&self) -> i64 {
        if self.points.len() < 2 {
            return 0;
        }
        self.points.last().unwrap().ts_unix - self.points.first().unwrap().ts_unix
    }

    /// Max segment speed in m/s. Returns None for degenerate tracks.
    pub fn max_segment_speed_mps(&self) -> Option<f64> {
        let mut best: Option<f64> = None;
        for w in self.points.windows(2) {
            let dt = (w[1].ts_unix - w[0].ts_unix) as f64;
            if dt <= 0.0 {
                continue;
            }
            let v = w[0].pos.distance_m(&w[1].pos) / dt;
            best = Some(best.map_or(v, |b: f64| b.max(v)));
        }
        best
    }

    /// Douglas-Peucker simplification with tolerance in meters.
    pub fn simplify(&self, tol_m: f64) -> Track {
        if self.points.len() < 3 {
            return self.clone();
        }
        let pts: Vec<&LatLon> = self.points.iter().map(|p| &p.pos).collect();
        let mut keep = vec![false; pts.len()];
        keep[0] = true;
        keep[pts.len() - 1] = true;
        let mut stack = vec![(0usize, pts.len() - 1)];
        while let Some((a, b)) = stack.pop() {
            let mut dmax = 0.0;
            let mut idx = a;
            for i in a + 1..b {
                let d = perp_distance(&pts[i], &pts[a], &pts[b]);
                if d > dmax {
                    dmax = d;
                    idx = i;
                }
            }
            if dmax > tol_m {
                keep[idx] = true;
                stack.push((a, idx));
                stack.push((idx, b));
            }
        }
        let mut out = Track::new(&self.label);
        for (i, p) in self.points.iter().enumerate() {
            if keep[i] {
                out.push(p.clone());
            }
        }
        out
    }

    pub fn to_geojson(&self) -> String {
        let mut fc = FeatureCollection::new();
        let line: Vec<LatLon> = self.points.iter().map(|p| p.pos).collect();
        fc.push(
            Feature::new(Geometry::LineString(line))
                .prop("name", &self.label)
                .prop("distance_m", &format!("{:.1}", self.total_distance_m()))
                .prop("duration_s", &self.duration_s().to_string()),
        );
        fc.to_json()
    }

    /// Render the track onto a raster tile (x,y,z) with a 2px polyline.
    pub fn overlay_tile(&self, raster: &mut Raster, tile_x: u32, tile_y: u32, z: u8, color: [u8; 3]) {
        let (nw, _se) = WebMercator::tile_bbox(tile_x, tile_y, z);
        let n = 2f64.powi(z as i32);
        for tp in &self.points {
            let fx = (tp.pos.lon + 180.0) / 360.0 * n - tile_x as f64;
            let lat = tp.pos.lat.clamp(-WebMercator::MAX_LAT, WebMercator::MAX_LAT).to_radians();
            let fy = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n
                - tile_y as f64;
            let px = (fx * 256.0) as i64;
            let py = (fy * 256.0) as i64;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if px + dx >= 0 && py + dy >= 0 {
                        raster.set_px((px + dx) as u32, (py + dy) as u32, color);
                    }
                }
            }
        }
        let _ = nw; // bbox origin already consumed via tile_xy math
    }
}

fn perp_distance(p: &LatLon, a: &LatLon, b: &LatLon) -> f64 {
    // Approximate planar distance via local equirectangular projection.
    let kx = 111_320.0 * a.lat.to_radians().cos();
    let ky = 110_540.0;
    let ax = a.lon * kx;
    let ay = a.lat * ky;
    let bx = b.lon * kx;
    let by = b.lat * ky;
    let px = p.lon * kx;
    let py = p.lat * ky;
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    ((px - (ax + t * dx)).powi(2) + (py - (ay + t * dy)).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track() -> Track {
        let mut t = Track::new("ganges-loop");
        let base = 1_790_000_000i64;
        let pts = [(48.8541, -123.4994), (48.8600, -123.4900), (48.8700, -123.4800), (48.8800, -123.4700)];
        for (i, (lat, lon)) in pts.iter().enumerate() {
            t.push(TrackPoint { pos: LatLon::new_unchecked(*lat, *lon), ts_unix: base + i as i64 * 300 });
        }
        t
    }

    #[test]
    fn distance_and_duration() {
        let t = sample_track();
        assert!(t.total_distance_m() > 3_000.0);
        assert_eq!(t.duration_s(), 900);
    }

    #[test]
    fn max_speed_sane_pedicab() {
        let t = sample_track();
        let v = t.max_segment_speed_mps().unwrap();
        assert!(v > 0.5 && v < 30.0, "v={v}");
    }

    #[test]
    fn simplify_reduces_points() {
        let mut t = Track::new("line");
        for i in 0..50 {
            t.push(TrackPoint {
                pos: LatLon::new_unchecked(48.85 + i as f64 * 0.001, -123.49),
                ts_unix: i as i64,
            });
        }
        let s = t.simplify(5.0);
        assert!(s.points.len() < t.points.len());
        assert_eq!(s.points.len(), 2); // perfectly straight
    }

    #[test]
    fn overlay_paints_pixels() {
        let t = sample_track();
        let mut r = Raster::new(256, 256);
        let (x, y) = WebMercator::tile_xy(&t.points[0].pos, 12);
        t.overlay_tile(&mut r, x, y, 12, [255, 0, 0]);
        let painted = r.data.chunks(3).any(|p| p == [255, 0, 0]);
        assert!(painted);
    }
}
