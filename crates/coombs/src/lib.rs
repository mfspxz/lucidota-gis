//! coombs — gis-003: Coombs GIS core (Coombs / Parksville / Errington, BC).
//!
//! Done-def: map; layers; offline. Every dataset is tagged offline_capable and
//! the crate emits an offline manifest consumed by offline_packs (gis-009).

use geo_core::coord::LatLon;
use geo_core::geojson::{Feature, FeatureCollection, Geometry};
use geo_core::validate::Region;

pub fn region() -> Region {
    Region {
        name: "coombs-parksville".into(),
        min_lat: 49.22,
        max_lat: 49.38,
        min_lon: -124.65,
        max_lon: -124.22,
    }
}

#[derive(Debug, Clone)]
pub struct Place {
    pub name: &'static str,
    pub kind: &'static str,
    pub pos: LatLon,
    pub note: &'static str,
    pub offline_capable: bool,
}

pub fn places() -> Vec<Place> {
    vec![
        Place { name: "Coombs Old Country Market", kind: "attraction", pos: LatLon::new_unchecked(49.3063, -124.4223), note: "Goats on the roof", offline_capable: true },
        Place { name: "Coombs Junction", kind: "village", pos: LatLon::new_unchecked(49.3058, -124.4283), note: "Hwy 4 / Alberni Hwy junction", offline_capable: true },
        Place { name: "Parksville", kind: "town", pos: LatLon::new_unchecked(49.3192, -124.3189), note: "Rathtrevor, beaches", offline_capable: true },
        Place { name: "Errington", kind: "village", pos: LatLon::new_unchecked(49.2907, -124.3470), note: "Englishman River Falls Rd", offline_capable: true },
        Place { name: "Rathtrevor Beach", kind: "park", pos: LatLon::new_unchecked(49.3264, -124.2706), note: "Provincial park, tidal flats", offline_capable: true },
        Place { name: "Englishman River Falls", kind: "park", pos: LatLon::new_unchecked(49.2350, -124.3520), note: "Provincial park, falls + trails", offline_capable: true },
        Place { name: "Little Qualicum Falls", kind: "park", pos: LatLon::new_unchecked(49.3136, -124.5670), note: "Provincial park (west edge of region)", offline_capable: true },
        Place { name: "French Creek", kind: "waterway", pos: LatLon::new_unchecked(49.3320, -124.3600), note: "Marina, estuary", offline_capable: true },
        Place { name: "Coombs Fairgrounds", kind: "venue", pos: LatLon::new_unchecked(49.3010, -124.4150), note: "Arrowsmith exhibition grounds (approx)", offline_capable: true },
        Place { name: "Butterfly World", kind: "attraction", pos: LatLon::new_unchecked(49.3070, -124.4180), note: "Next to the market (approx)", offline_capable: true },
    ]
}

pub fn layers_geojson() -> Vec<(String, String)> {
    let mut fc = FeatureCollection::new();
    for pl in places() {
        fc.push(
            Feature::new(Geometry::Point(pl.pos))
                .prop("name", pl.name)
                .prop("kind", pl.kind)
                .prop("note", pl.note)
                .prop("offline_capable", if pl.offline_capable { "true" } else { "false" }),
        );
    }
    vec![("coombs_places".into(), fc.to_json())]
}

/// Offline manifest: region bbox + zoom range + layer names for pack building.
pub fn offline_manifest() -> String {
    let r = region();
    format!(
        "{{\"region\":\"{}\",\"bbox\":[{:.4},{:.4},{:.4},{:.4}],\"zooms\":[8,14],\"layers\":[\"coombs_places\"],\"offline_capable\":true}}",
        r.name, r.min_lat, r.min_lon, r.max_lat, r.max_lon
    )
}

pub fn validate_all() -> Vec<String> {
    let r = region();
    let mut errs = Vec::new();
    for pl in places() {
        if let geo_core::validate::Verdict::Reject(msg) = r.check_point(&pl.pos) {
            errs.push(format!("place {}: {msg}", pl.name));
        }
        if !pl.offline_capable {
            errs.push(format!("place {}: not offline-capable", pl.name));
        }
    }
    errs
}
