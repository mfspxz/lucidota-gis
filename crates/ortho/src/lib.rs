//! ortho — gis-007: orthomosaic pipeline, drone imagery -> tiles.
//!
//! Ingest format: georeferenced PPM P6 frames (documented; convert from
//! camera JPEG/PNG with any ppm tool). Each frame carries its NW corner and
//! ground sample distance. The pipeline mosaics frames at a target zoom,
//! builds a downsampled tile pyramid, and emits PNG tiles.

use geo_core::coord::{LatLon, WebMercator};
use geo_core::raster::Raster;
use geo_core::tile::Tile;

/// One georeferenced drone frame.
#[derive(Debug, Clone)]
pub struct Frame {
    pub path: String,
    pub nw: LatLon,        // northwest corner of the frame
    pub pixel_size_m: f64, // ground sample distance
}

/// Mosaic canvas at a fixed zoom.
#[derive(Debug)]
pub struct Mosaic {
    pub z: u8,
    pub origin_x: u32, // tile x of canvas origin
    pub origin_y: u32, // tile y of canvas origin
    pub raster: Raster,
    pub coverage: Raster, // per-pixel sample counts (u8 per channel reused as count in R)
}

impl Mosaic {
    fn px_of(&self, p: &LatLon) -> Option<(i64, i64)> {
        let n = 2f64.powi(self.z as i32);
        let fx = (p.lon + 180.0) / 360.0 * n;
        let lat = p.lat.clamp(-WebMercator::MAX_LAT, WebMercator::MAX_LAT).to_radians();
        let fy = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
        let px = (fx * 256.0) as i64 - self.origin_x as i64 * 256;
        let py = (fy * 256.0) as i64 - self.origin_y as i64 * 256;
        Some((px, py))
    }

    fn blend_px(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
        let i = (y as usize * self.raster.width as usize + x as usize) * 3;
        let n = self.coverage.data[i] as u32;
        let nn = n + 1;
        for c in 0..3 {
            let acc = self.raster.data[i + c] as u32 * n + rgb[c] as u32;
            self.raster.data[i + c] = (acc / nn) as u8;
        }
        let nv = nn.min(255) as u8;
        self.coverage.data[i] = nv;
        self.coverage.data[i + 1] = nv;
        self.coverage.data[i + 2] = nv;
    }
}

/// Build a mosaic canvas covering all frames at zoom z.
pub fn mosaic_frames(frames: &[Frame], z: u8) -> std::io::Result<Mosaic> {
    assert!(!frames.is_empty(), "no frames");
    // Determine tile coverage from frame corners.
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut loaded: Vec<(Frame, Raster)> = Vec::new();
    for f in frames {
        let r = Raster::read_ppm(&f.path)?;
        let se = frame_se_corner(f, r.width, r.height);
        let (x0, y0) = WebMercator::tile_xy(&f.nw, z);
        let (x1, y1) = WebMercator::tile_xy(&se, z);
        xs.extend([x0, x1]);
        ys.extend([y0, y1]);
        loaded.push((f.clone(), r));
    }
    let (x0, x1) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
    let (y0, y1) = (*ys.iter().min().unwrap(), *ys.iter().max().unwrap());
    let w = (x1 - x0 + 1) * 256;
    let h = (y1 - y0 + 1) * 256;
    let mut m = Mosaic {
        z,
        origin_x: x0,
        origin_y: y0,
        raster: Raster::new(w, h),
        coverage: Raster::new(w, h),
    };
    for (f, r) in &loaded {
        for py in 0..r.height {
            for px in 0..r.width {
                // pixel center -> ground offset -> latlon
                let east_m = (px as f64 + 0.5) * f.pixel_size_m;
                let south_m = (py as f64 + 0.5) * f.pixel_size_m;
                let p = f.nw.destination(180.0, south_m).destination(90.0, east_m);
                if let Some((cx, cy)) = m.px_of(&p) {
                    if cx >= 0 && cy >= 0 && cx < w as i64 && cy < h as i64 {
                        m.blend_px(cx as u32, cy as u32, r.get_px(px, py));
                    }
                }
            }
        }
    }
    Ok(m)
}

fn frame_se_corner(f: &Frame, w: u32, h: u32) -> LatLon {
    let east_m = w as f64 * f.pixel_size_m;
    let south_m = h as f64 * f.pixel_size_m;
    f.nw.destination(180.0, south_m).destination(90.0, east_m)
}

#[derive(Debug)]
pub struct PyramidTile {
    pub tile: Tile,
    pub png: Vec<u8>,
}

/// Cut a tile pyramid from z_min..=z_max. z_max is the mosaic zoom; lower
/// zooms are produced by successive downsampling.
pub fn build_pyramid(m: &Mosaic, z_min: u8, z_max: u8) -> Vec<PyramidTile> {
    assert!(z_max == m.z, "mosaic zoom must equal z_max");
    assert!(z_min <= z_max);
    let mut out = Vec::new();
    let mut level = LevelRaster {
        z: z_max,
        ox: m.origin_x,
        oy: m.origin_y,
        raster: m.raster.clone(),
    };
    loop {
        let tw = level.raster.width.div_ceil(256).max(1);
        let th = level.raster.height.div_ceil(256).max(1);
        for ty in 0..th {
            for tx in 0..tw {
                let mut tile = Raster::new(256, 256);
                for y in 0..256 {
                    for x in 0..256 {
                        let sx = (tx * 256 + x).min(level.raster.width - 1);
                        let sy = (ty * 256 + y).min(level.raster.height - 1);
                        tile.set_px(x, y, level.raster.get_px(sx, sy));
                    }
                }
                let mut png = Vec::new();
                tile.write_png(&mut png).expect("png encode");
                out.push(PyramidTile {
                    tile: Tile::new(level.ox + tx, level.oy + ty, level.z).unwrap(),
                    png,
                });
            }
        }
        if level.z == z_min {
            break;
        }
        level = LevelRaster {
            z: level.z - 1,
            ox: level.ox / 2,
            oy: level.oy / 2,
            raster: level.raster.downscale_half(),
        };
    }
    out
}

struct LevelRaster {
    z: u8,
    ox: u32,
    oy: u32,
    raster: Raster,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_frame(path: &str, nw: LatLon, color: [u8; 3]) -> Frame {
        let r = Raster::filled(64, 64, color[0], color[1], color[2]);
        r.write_ppm(path).unwrap();
        Frame { path: path.into(), nw, pixel_size_m: 2.0 }
    }

    #[test]
    fn mosaic_covers_two_frames() {
        let f1 = synth_frame("/tmp/ortho_f1.ppm", LatLon::new_unchecked(48.8600, -123.5000), [255, 0, 0]);
        let f2 = synth_frame("/tmp/ortho_f2.ppm", LatLon::new_unchecked(48.8590, -123.4990), [0, 0, 255]);
        let m = mosaic_frames(&[f1, f2], 16).unwrap();
        assert_eq!(m.z, 16);
        // both colors present in the mosaic
        let has_red = m.raster.data.chunks(3).any(|p| p[0] > 200 && p[1] < 50);
        let has_blue = m.raster.data.chunks(3).any(|p| p[2] > 200 && p[0] < 50);
        assert!(has_red && has_blue, "both frames must appear");
        // coverage recorded
        assert!(m.coverage.data.iter().any(|&c| c > 0));
    }

    #[test]
    fn pyramid_tile_counts_and_png_magic() {
        let f1 = synth_frame("/tmp/ortho_p1.ppm", LatLon::new_unchecked(48.8600, -123.5000), [10, 200, 10]);
        let m = mosaic_frames(&[f1], 14).unwrap();
        let tiles = build_pyramid(&m, 13, 14);
        // z14: canvas tiles; z13: quarter as many (canvas dims are multiples of 256)
        let z14 = tiles.iter().filter(|t| t.tile.z == 14).count();
        let z13 = tiles.iter().filter(|t| t.tile.z == 13).count();
        assert!(z14 >= 1);
        assert!(z13 >= 1);
        assert!(z13 <= z14, "z14={z14} z13={z13}");
        for t in &tiles {
            assert_eq!(&t.png[1..4], b"PNG", "valid PNG magic");
        }
    }

    #[test]
    fn empty_frames_rejected() {
        let r = std::panic::catch_unwind(|| mosaic_frames(&[], 14));
        assert!(r.is_err(), "empty frame list must panic, not silently succeed");
    }
}
