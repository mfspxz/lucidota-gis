//! coombs_data — gis-004: Coombs parcels + imagery manifest.
//!
//! Parcels: a deterministic sample fabric over the region (documented as a
//! synthetic stand-in until the DataBC parcel WFS mirror lands — gis-016).
//! Imagery manifest: expected orthomosaic tile coverage per zoom, with digest
//! slots filled by the ortho pipeline (gis-007) / offline packs (gis-009).

use geo_core::coord::LatLon;
use geo_core::geojson::{Feature, FeatureCollection, Geometry};
use geo_core::tile::Tile;

#[derive(Debug, Clone)]
pub struct Parcel {
    pub id: String,
    pub nw: LatLon,
    pub se: LatLon,
    pub land_use: &'static str,
    pub area_m2: f64,
}

const USES: [&str; 5] = ["residential", "agricultural", "commercial", "park", "forest"];

/// Deterministic grid fabric: nx by ny parcels across the region.
pub fn parcel_fabric(nx: u32, ny: u32) -> Vec<Parcel> {
    let r = coombs::region();
    let dlat = (r.max_lat - r.min_lat) / ny as f64;
    let dlon = (r.max_lon - r.min_lon) / nx as f64;
    let mut out = Vec::new();
    for iy in 0..ny {
        for ix in 0..nx {
            let nw = LatLon::new_unchecked(r.max_lat - iy as f64 * dlat, r.min_lon + ix as f64 * dlon);
            let se = LatLon::new_unchecked(r.max_lat - (iy + 1) as f64 * dlat, r.min_lon + (ix + 1) as f64 * dlon);
            let w_m = nw.distance_m(&LatLon::new_unchecked(nw.lat, se.lon));
            let h_m = nw.distance_m(&LatLon::new_unchecked(se.lat, nw.lon));
            // deterministic land use from grid hash (stable across runs)
            let h = (ix as u64 * 73856093) ^ (iy as u64 * 19349663);
            out.push(Parcel {
                id: format!("COOMB-{ix:02}-{iy:02}"),
                nw,
                se,
                land_use: USES[(h % USES.len() as u64) as usize],
                area_m2: w_m * h_m,
            });
        }
    }
    out
}

pub fn parcels_geojson(parcels: &[Parcel]) -> String {
    let mut fc = FeatureCollection::new();
    for p in parcels {
        let ring = vec![
            p.nw,
            LatLon::new_unchecked(p.nw.lat, p.se.lon),
            p.se,
            LatLon::new_unchecked(p.se.lat, p.nw.lon),
            p.nw,
        ];
        fc.push(
            Feature::new(Geometry::Polygon(vec![ring]))
                .prop("id", &p.id)
                .prop("land_use", p.land_use)
                .prop("area_m2", &format!("{:.0}", p.area_m2))
                .prop("fabric", "synthetic-sample"),
        );
    }
    fc.to_json()
}

#[derive(Debug, Clone)]
pub struct ImagerySlot {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub digest: Option<String>, // filled by ortho pipeline
}

/// Expected tile coverage for zooms 8..=14, digests pending.
pub fn imagery_manifest() -> Vec<ImagerySlot> {
    let r = coombs::region();
    let nw = LatLon::new_unchecked(r.max_lat, r.min_lon);
    let se = LatLon::new_unchecked(r.min_lat, r.max_lon);
    let mut out = Vec::new();
    for z in 8..=14u8 {
        for t in Tile::covering(&nw, &se, z) {
            out.push(ImagerySlot { z: t.z, x: t.x, y: t.y, digest: None });
        }
    }
    out
}

pub fn manifest_json(slots: &[ImagerySlot]) -> String {
    let mut per_zoom = std::collections::BTreeMap::<u8, usize>::new();
    for s in slots {
        *per_zoom.entry(s.z).or_default() += 1;
    }
    let zj: Vec<String> = per_zoom.iter().map(|(z, n)| format!("\"{z}\":{n}")).collect();
    format!(
        "{{\"region\":\"coombs-parksville\",\"tiles\":{},\"per_zoom\":{{{}}},\"digests_filled\":0,\"source\":\"ortho-pipeline-pending\"}}",
        slots.len(),
        zj.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_shape_and_ids() {
        let p = parcel_fabric(4, 3);
        assert_eq!(p.len(), 12);
        assert_eq!(p[0].id, "COOMB-00-00");
        assert!(p.iter().all(|x| x.area_m2 > 1000.0), "parcels must have real area");
        let uses: std::collections::HashSet<_> = p.iter().map(|x| x.land_use).collect();
        assert!(uses.len() > 1, "fabric should vary land use");
    }

    #[test]
    fn fabric_deterministic() {
        let a = parcel_fabric(5, 5);
        let b = parcel_fabric(5, 5);
        assert_eq!(a[7].land_use, b[7].land_use);
        assert!((a[7].area_m2 - b[7].area_m2).abs() < 1e-6);
    }

    #[test]
    fn parcels_geojson_valid_shape() {
        let gj = parcels_geojson(&parcel_fabric(2, 2));
        assert!(gj.contains("\"type\":\"Polygon\""));
        assert!(gj.contains("COOMB-01-01"));
        assert!(gj.contains("synthetic-sample"));
    }

    #[test]
    fn imagery_manifest_pyramid_grows() {
        let slots = imagery_manifest();
        let z8 = slots.iter().filter(|s| s.z == 8).count();
        let z14 = slots.iter().filter(|s| s.z == 14).count();
        assert!(z14 > z8, "z8={z8} z14={z14}");
        assert!(slots.iter().all(|s| s.digest.is_none()));
        let j = manifest_json(&slots);
        assert!(j.contains("\"tiles\":"));
    }
}
