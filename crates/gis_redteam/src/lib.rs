//! gis_redteam — gis-014: adversarial battery against the GIS ingest boundary.
//!
//! Every attack MUST be rejected; every honest input MUST be accepted (no
//! over-correction). The battery runs the same checks production ingest uses.

use geo_core::coord::LatLon;
use geo_core::track::{Track, TrackPoint};
use geo_core::validate::{Region, Verdict};

pub fn saltspring_region() -> Region {
    Region {
        name: "saltspring-island".into(),
        min_lat: 48.70, max_lat: 49.00, min_lon: -123.75, max_lon: -123.25,
    }
}

/// Attack outcome: (name, rejected?, detail).
#[derive(Debug)]
pub struct AttackResult {
    pub name: &'static str,
    pub rejected: bool,
    pub detail: String,
}

static BATTERY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn run_battery() -> Vec<AttackResult> {
    let r = saltspring_region();
    let mut out = Vec::new();
    // unique temp paths: tests run run_battery() concurrently
    let seq = BATTERY_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    // A1: NaN injection
    let v = r.check_point(&LatLon::new_unchecked(f64::NAN, -123.49));
    out.push(AttackResult { name: "nan-coordinate", rejected: !v.ok(), detail: format!("{v:?}") });

    // A2: out-of-region point smuggled as local (Vancouver)
    let v = r.check_point(&LatLon::new_unchecked(49.2827, -123.1207));
    out.push(AttackResult { name: "out-of-region-point", rejected: !v.ok(), detail: format!("{v:?}") });

    // A3: teleport spoof (10km in 10s)
    let mut t = Track::new("spoof");
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.85, -123.49), ts_unix: 1000 });
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.95, -123.40), ts_unix: 1010 });
    let v = r.check_track(&t, 25.0);
    out.push(AttackResult { name: "teleport-spoof", rejected: !v.ok(), detail: format!("{v:?}") });

    // A4: timestamp replay
    let mut t = Track::new("replay");
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.85, -123.49), ts_unix: 2000 });
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.86, -123.49), ts_unix: 1000 });
    let v = r.check_track(&t, 25.0);
    out.push(AttackResult { name: "timestamp-replay", rejected: !v.ok(), detail: format!("{v:?}") });

    // A5: tampered pack tile
    let p = format!("/tmp/redteam_pack_{seq}.gispack");
    let png = { let r = geo_core::raster::Raster::filled(8, 8, 1, 2, 3); let mut b = Vec::new(); r.write_png(&mut b).unwrap(); b };
    let mut b = offline_packs::PackBuilder::new("rt", "{}");
    b.add_tile(10, 1, 2, png);
    b.build(&p).unwrap();
    let mut info = offline_packs::PackReader::open(&p).unwrap();
    info.tiles[0].png[50] ^= 0xff;
    let v = offline_packs::PackReader::verify(&info);
    out.push(AttackResult { name: "tampered-pack-tile", rejected: v.is_err(), detail: format!("{v:?}").chars().take(80).collect() });

    // A6: CAS digest mismatch (forged bytes under honest digest)
    let dir = std::env::temp_dir().join(format!("redteam_cas_{seq}"));
    let _ = std::fs::remove_dir_all(&dir);
    let store = palontour::ContentStore::open(&dir).unwrap();
    let d = store.put(b"honest-bytes").unwrap();
    let v = store.verify(&d, b"forged-bytes");
    out.push(AttackResult { name: "cas-digest-mismatch", rejected: !v, detail: format!("verify={v}") });

    // H1: honest track accepted (no over-correction)
    let mut t = Track::new("honest-ride");
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.8541, -123.4994), ts_unix: 1000 });
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.8600, -123.4900), ts_unix: 1600 });
    let v = r.check_track(&t, 25.0);
    out.push(AttackResult { name: "honest-track", rejected: !v.ok(), detail: format!("{v:?}") });

    // H2: honest pack verifies
    let info = offline_packs::PackReader::open(&p).unwrap();
    let v = offline_packs::PackReader::verify(&info);
    out.push(AttackResult { name: "honest-pack", rejected: v.is_err(), detail: format!("{v:?}") });

    // H3: boundary point exactly on region edge accepted
    let v = r.check_point(&LatLon::new_unchecked(48.70, -123.50));
    out.push(AttackResult { name: "boundary-point", rejected: !v.ok(), detail: format!("{v:?}") });

    // H4: degenerate single-point track accepted
    let mut t = Track::new("single");
    t.push(TrackPoint { pos: LatLon::new_unchecked(48.85, -123.49), ts_unix: 1000 });
    let v = r.check_track(&t, 25.0);
    out.push(AttackResult { name: "single-point-track", rejected: !v.ok(), detail: format!("{v:?}") });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_attacks_rejected() {
        for a in run_battery().iter().filter(|a| !a.name.starts_with("honest") && !a.name.starts_with("boundary") && !a.name.starts_with("single")) {
            assert!(a.rejected, "ATTACK ACCEPTED: {} ({})", a.name, a.detail);
        }
    }

    #[test]
    fn honest_inputs_accepted_no_overcorrection() {
        for a in run_battery().iter().filter(|a| a.name.starts_with("honest") || a.name.starts_with("boundary") || a.name.starts_with("single")) {
            assert!(!a.rejected, "HONEST INPUT REJECTED: {} ({})", a.name, a.detail);
        }
    }

    #[test]
    fn battery_covers_ten_cases() {
        assert_eq!(run_battery().len(), 10);
    }
}
