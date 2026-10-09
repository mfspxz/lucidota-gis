//! Data validation: reject spoofed / impossible / out-of-region geodata.
//!
//! This is the shared core behind gis-014 (GIS red-team): every ingest path
//! runs through these checks. Checks return a verdict, never a panic.

use crate::coord::LatLon;
use crate::track::Track;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Accept,
    Reject(String),
}

impl Verdict {
    pub fn ok(&self) -> bool {
        matches!(self, Verdict::Accept)
    }
}

/// Region of interest: everything outside is rejected as out-of-scope.
#[derive(Debug, Clone)]
pub struct Region {
    pub name: String,
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
}

impl Region {
    pub fn contains(&self, p: &LatLon) -> bool {
        p.lat >= self.min_lat
            && p.lat <= self.max_lat
            && p.lon >= self.min_lon
            && p.lon <= self.max_lon
    }

    pub fn check_point(&self, p: &LatLon) -> Verdict {
        if !p.lat.is_finite() || !p.lon.is_finite() {
            return Verdict::Reject("non-finite coordinate (NaN/inf injection)".into());
        }
        if !self.contains(p) {
            return Verdict::Reject(format!(
                "point ({:.4},{:.4}) outside region {}",
                p.lat, p.lon, self.name
            ));
        }
        Verdict::Accept
    }

    /// Track-level checks: containment, max plausible speed (spoofed jumps),
    /// monotonic timestamps.
    pub fn check_track(&self, t: &Track, max_speed_mps: f64) -> Verdict {
        for tp in &t.points {
            let v = self.check_point(&tp.pos);
            if !v.ok() {
                return v;
            }
        }
        for w in t.points.windows(2) {
            if w[1].ts_unix < w[0].ts_unix {
                return Verdict::Reject("non-monotonic timestamps (replay/spoof)".into());
            }
            let dt = (w[1].ts_unix - w[0].ts_unix) as f64;
            if dt > 0.0 {
                let v = w[0].pos.distance_m(&w[1].pos) / dt;
                if v > max_speed_mps {
                    return Verdict::Reject(format!(
                        "impossible speed {v:.1} m/s > {max_speed_mps} (teleport spoof)"
                    ));
                }
            }
        }
        Verdict::Accept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::TrackPoint;

    fn saltspring() -> Region {
        Region {
            name: "saltspring".into(),
            min_lat: 48.70,
            max_lat: 49.00,
            min_lon: -123.75,
            max_lon: -123.25,
        }
    }

    #[test]
    fn rejects_nan_and_outside() {
        let r = saltspring();
        assert!(!r.check_point(&LatLon::new_unchecked(f64::NAN, 0.0)).ok());
        assert!(!r.check_point(&LatLon::new_unchecked(49.28, -123.12)).ok()); // Vancouver
        assert!(r.check_point(&LatLon::new_unchecked(48.85, -123.49)).ok());
    }

    #[test]
    fn rejects_teleport_spoof() {
        let r = saltspring();
        let mut t = Track::new("spoof");
        t.push(TrackPoint { pos: LatLon::new_unchecked(48.85, -123.49), ts_unix: 1000 });
        // ~10 km in 10 s = 1000 m/s: physically impossible for a pedicab.
        t.push(TrackPoint { pos: LatLon::new_unchecked(48.95, -123.40), ts_unix: 1010 });
        let v = r.check_track(&t, 25.0);
        assert!(!v.ok(), "spoof accepted: {v:?}");
        assert!(format!("{v:?}").contains("impossible speed"));
    }

    #[test]
    fn rejects_time_travel() {
        let r = saltspring();
        let mut t = Track::new("replay");
        t.push(TrackPoint { pos: LatLon::new_unchecked(48.85, -123.49), ts_unix: 2000 });
        t.push(TrackPoint { pos: LatLon::new_unchecked(48.86, -123.49), ts_unix: 1000 });
        assert!(!r.check_track(&t, 25.0).ok());
    }

    #[test]
    fn accepts_honest_track() {
        let r = saltspring();
        let mut t = Track::new("ride");
        t.push(TrackPoint { pos: LatLon::new_unchecked(48.8541, -123.4994), ts_unix: 1000 });
        t.push(TrackPoint { pos: LatLon::new_unchecked(48.8600, -123.4900), ts_unix: 1600 });
        assert!(r.check_track(&t, 25.0).ok());
    }
}
