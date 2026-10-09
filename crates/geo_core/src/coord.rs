//! Coordinate primitives: LatLon, Web Mercator projection, haversine geodesy.

/// WGS-84 latitude/longitude point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    /// Checked constructor: rejects out-of-range values (NaN included).
    pub fn new(lat: f64, lon: f64) -> Option<Self> {
        if !lat.is_finite() || !lon.is_finite() {
            return None;
        }
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return None;
        }
        Some(Self { lat, lon })
    }

    pub fn new_unchecked(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Haversine great-circle distance in meters.
    pub fn distance_m(&self, other: &LatLon) -> f64 {
        let r = 6_371_008.8f64;
        let a1 = self.lat.to_radians();
        let a2 = other.lat.to_radians();
        let h = ((a2 - a1) / 2.0).sin().powi(2)
            + a1.cos() * a2.cos() * (((other.lon - self.lon).to_radians()) / 2.0).sin().powi(2);
        2.0 * r * h.sqrt().asin()
    }

    /// Initial bearing in degrees clockwise from north.
    pub fn bearing_deg(&self, other: &LatLon) -> f64 {
        let a1 = self.lat.to_radians();
        let a2 = other.lat.to_radians();
        let dl = (other.lon - self.lon).to_radians();
        let y = dl.sin() * a2.cos();
        let x = a1.cos() * a2.sin() - a1.sin() * a2.cos() * dl.cos();
        (y.atan2(x).to_degrees() + 360.0) % 360.0
    }

    /// Destination point given bearing (deg) and distance (m).
    pub fn destination(&self, bearing_deg: f64, dist_m: f64) -> LatLon {
        let r = 6_371_008.8f64;
        let d = dist_m / r;
        let b = bearing_deg.to_radians();
        let a1 = self.lat.to_radians();
        let a2 = (a1.sin() * d.cos() + a1.cos() * d.sin() * b.cos()).asin();
        let l2 = self.lon.to_radians()
            + (b.sin() * d.sin() * a1.cos()).atan2(d.cos() - a1.sin() * a2.sin());
        LatLon::new_unchecked(a2.to_degrees(), l2.to_degrees())
    }
}

/// Web Mercator (EPSG:3857) projection + slippy-map tile math.
pub struct WebMercator;

impl WebMercator {
    pub const R: f64 = 6_378_137.0;
    pub const MAX_LAT: f64 = 85.05112878;

    /// lon/lat -> meters.
    pub fn to_meters(p: &LatLon) -> (f64, f64) {
        let x = Self::R * p.lon.to_radians();
        let lat = p.lat.clamp(-Self::MAX_LAT, Self::MAX_LAT).to_radians();
        let y = Self::R * ((std::f64::consts::FRAC_PI_4 + lat / 2.0).tan()).ln();
        (x, y)
    }

    /// meters -> lon/lat.
    pub fn to_latlon(x: f64, y: f64) -> LatLon {
        let lon = (x / Self::R).to_degrees();
        let lat = (2.0 * (y / Self::R).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
        LatLon::new_unchecked(lat, lon)
    }

    /// Slippy-map tile x/y at zoom z.
    pub fn tile_xy(p: &LatLon, z: u8) -> (u32, u32) {
        let n = 2u32.pow(z as u32) as f64;
        let lat = p.lat.clamp(-Self::MAX_LAT, Self::MAX_LAT).to_radians();
        let mut x = ((p.lon + 180.0) / 360.0 * n).floor() as i64;
        let mut y = ((1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
            .floor() as i64;
        let m = n as i64;
        x = x.clamp(0, m - 1);
        y = y.clamp(0, m - 1);
        (x as u32, y as u32)
    }

    /// Tile bounding box as (northwest, southeast) LatLon.
    pub fn tile_bbox(x: u32, y: u32, z: u8) -> (LatLon, LatLon) {
        fn num(x: u32, y: u32, z: u8) -> (f64, f64) {
            let n = 2f64.powi(z as i32);
            let lon = x as f64 / n * 360.0 - 180.0;
            let lat = (std::f64::consts::PI * (1.0 - 2.0 * y as f64 / n))
                .sinh()
                .atan()
                .to_degrees();
            (lat, lon)
        }
        let (nlat, wlon) = num(x, y, z);
        let (slat, elon) = num(x + 1, y + 1, z);
        (
            LatLon::new_unchecked(nlat, wlon),
            LatLon::new_unchecked(slat, elon),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latlon_checked_rejects_garbage() {
        assert!(LatLon::new(91.0, 0.0).is_none());
        assert!(LatLon::new(0.0, 181.0).is_none());
        assert!(LatLon::new(f64::NAN, 0.0).is_none());
        assert!(LatLon::new(48.85, -123.49).is_some());
    }

    #[test]
    fn haversine_ganges_to_fulford() {
        // Ganges 48.8541,-123.4994 ; Fulford Harbour 48.7677,-123.4500 — real-world ~10.3 km.
        let a = LatLon::new_unchecked(48.8541, -123.4994);
        let b = LatLon::new_unchecked(48.7677, -123.4500);
        let d = a.distance_m(&b);
        assert!((9_500.0..11_500.0).contains(&d), "d={d}");
    }

    #[test]
    fn bearing_cardinal() {
        let a = LatLon::new_unchecked(0.0, 0.0);
        let b = LatLon::new_unchecked(1.0, 0.0);
        assert!((a.bearing_deg(&b) - 0.0).abs() < 1e-6);
        let c = LatLon::new_unchecked(0.0, 1.0);
        assert!((a.bearing_deg(&c) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn mercator_roundtrip() {
        let p = LatLon::new_unchecked(48.8541, -123.4994);
        let (x, y) = WebMercator::to_meters(&p);
        let q = WebMercator::to_latlon(x, y);
        assert!((q.lat - p.lat).abs() < 1e-9 && (q.lon - p.lon).abs() < 1e-9);
    }

    #[test]
    fn tile_roundtrip_ganges() {
        let p = LatLon::new_unchecked(48.8541, -123.4994);
        let (x, y) = WebMercator::tile_xy(&p, 12);
        let (nw, se) = WebMercator::tile_bbox(x, y, 12);
        assert!(p.lat <= nw.lat && p.lat >= se.lat);
        assert!(p.lon >= nw.lon && p.lon <= se.lon);
    }

    #[test]
    fn destination_roundtrip() {
        let a = LatLon::new_unchecked(48.85, -123.49);
        let b = a.destination(45.0, 5_000.0);
        assert!((a.distance_m(&b) - 5_000.0).abs() < 1.0);
        assert!((a.bearing_deg(&b) - 45.0).abs() < 0.5);
    }
}
