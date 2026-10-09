//! Saltspring Island canonical dataset. Coordinates are real-world named
//! features (Ganges, ferry terminals, parks, lakes). Road polylines are
//! approximate connectors between real endpoints — labelled as such.

use geo_core::coord::LatLon;
use geo_core::geojson::{Feature, FeatureCollection, Geometry};
use geo_core::validate::Region;

pub fn region() -> Region {
    Region {
        name: "saltspring-island".into(),
        min_lat: 48.70,
        max_lat: 49.00,
        min_lon: -123.75,
        max_lon: -123.25,
    }
}

#[derive(Debug, Clone)]
pub struct Place {
    pub name: &'static str,
    pub kind: &'static str,
    pub pos: LatLon,
    pub note: &'static str,
}

pub fn places() -> Vec<Place> {
    vec![
        Place { name: "Ganges", kind: "village", pos: LatLon::new_unchecked(48.8541, -123.4994), note: "Main village; harbour, market, services" },
        Place { name: "Fulford Harbour", kind: "ferry", pos: LatLon::new_unchecked(48.7677, -123.4500), note: "BC Ferries -> Swartz Bay (Victoria)" },
        Place { name: "Long Harbour", kind: "ferry", pos: LatLon::new_unchecked(48.8594, -123.4750), note: "BC Ferries -> Tsawwassen" },
        Place { name: "Vesuvius Bay", kind: "ferry", pos: LatLon::new_unchecked(48.8862, -123.5572), note: "BC Ferries -> Crofton (Vancouver Island)" },
        Place { name: "Ruckle Provincial Park", kind: "park", pos: LatLon::new_unchecked(48.7589, -123.3800), note: "SE tip; shoreline camping" },
        Place { name: "Mount Maxwell", kind: "park", pos: LatLon::new_unchecked(48.8572, -123.5064), note: "Summit viewpoint, Burgoyne Bay overlook" },
        Place { name: "Beddis Beach", kind: "beach", pos: LatLon::new_unchecked(48.8319, -123.4530), note: "Swimming beach, east side" },
        Place { name: "Burgoyne Bay", kind: "park", pos: LatLon::new_unchecked(48.8120, -123.5230), note: "Provincial park, estuary" },
        Place { name: "St. Mary Lake", kind: "lake", pos: LatLon::new_unchecked(48.8742, -123.5120), note: "Ganges water supply" },
        Place { name: "Cusheon Lake", kind: "lake", pos: LatLon::new_unchecked(48.8230, -123.4750), note: "Swimming lake, SE of Ganges" },
        Place { name: "Fernwood", kind: "village", pos: LatLon::new_unchecked(48.8950, -123.5050), note: "North-end dock community" },
        Place { name: "Fulford Village", kind: "village", pos: LatLon::new_unchecked(48.7700, -123.4480), note: "South-end village at ferry" },
        Place { name: "Salt Spring Island Airstrip", kind: "airstrip", pos: LatLon::new_unchecked(48.8620, -123.5000), note: "Private grass strip (approx)" },
        Place { name: "Mouat Park", kind: "park", pos: LatLon::new_unchecked(48.8480, -123.5050), note: "Ganges community park" },
    ]
}

#[derive(Debug, Clone)]
pub struct FerryRoute {
    pub name: &'static str,
    pub from: &'static str,
    pub to: &'static str,
    pub operator: &'static str,
    pub waypoints: Vec<LatLon>,
}

pub fn ferry_routes() -> Vec<FerryRoute> {
    vec![
        FerryRoute {
            name: "Fulford-Swartz Bay", from: "Fulford Harbour", to: "Swartz Bay",
            operator: "BC Ferries",
            waypoints: vec![
                LatLon::new_unchecked(48.7677, -123.4500),
                LatLon::new_unchecked(48.7200, -123.4300),
                LatLon::new_unchecked(48.6890, -123.4070), // Swartz Bay approx
            ],
        },
        FerryRoute {
            name: "Long Harbour-Tsawwassen", from: "Long Harbour", to: "Tsawwassen",
            operator: "BC Ferries",
            waypoints: vec![
                LatLon::new_unchecked(48.8594, -123.4750),
                LatLon::new_unchecked(48.9000, -123.3500),
                LatLon::new_unchecked(49.0080, -123.1280), // Tsawwassen approx
            ],
        },
        FerryRoute {
            name: "Vesuvius-Crofton", from: "Vesuvius Bay", to: "Crofton",
            operator: "BC Ferries",
            waypoints: vec![
                LatLon::new_unchecked(48.8862, -123.5572),
                LatLon::new_unchecked(48.8700, -123.6300),
                LatLon::new_unchecked(48.8620, -123.6900), // Crofton approx
            ],
        },
    ]
}

/// Approximate main-road connectors between real endpoints (documented approx).
pub fn roads() -> Vec<(&'static str, Vec<LatLon>)> {
    let p = |lat: f64, lon: f64| LatLon::new_unchecked(lat, lon);
    vec![
        ("Fulford-Ganges Rd", vec![p(48.7677, -123.4500), p(48.8000, -123.4700), p(48.8541, -123.4994)]),
        ("Vesuvius Bay Rd", vec![p(48.8862, -123.5572), p(48.8700, -123.5300), p(48.8541, -123.4994)]),
        ("Long Harbour Rd", vec![p(48.8594, -123.4750), p(48.8570, -123.4900), p(48.8541, -123.4994)]),
        ("North End Rd", vec![p(48.8541, -123.4994), p(48.8950, -123.5050)]),
        ("Stewart Rd / Beaver Pt Rd", vec![p(48.7677, -123.4500), p(48.7589, -123.3800)]),
    ]
}

/// Build all named layers as (layer_name, geojson).
pub fn layers_geojson() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut fc = FeatureCollection::new();
    for pl in places() {
        fc.push(
            Feature::new(Geometry::Point(pl.pos))
                .prop("name", pl.name)
                .prop("kind", pl.kind)
                .prop("note", pl.note),
        );
    }
    out.push(("places".into(), fc.to_json()));

    let mut fc = FeatureCollection::new();
    for r in ferry_routes() {
        fc.push(
            Feature::new(Geometry::LineString(r.waypoints))
                .prop("name", r.name)
                .prop("from", r.from)
                .prop("to", r.to)
                .prop("operator", r.operator),
        );
    }
    out.push(("ferry_routes".into(), fc.to_json()));

    let mut fc = FeatureCollection::new();
    for (name, line) in roads() {
        fc.push(
            Feature::new(Geometry::LineString(line))
                .prop("name", name)
                .prop("approx", "true"),
        );
    }
    out.push(("roads".into(), fc.to_json()));
    out
}

/// Validate every dataset point against the island region. Returns error strings.
pub fn validate_all() -> Vec<String> {
    let r = region();
    let mut errs = Vec::new();
    for pl in places() {
        if let geo_core::validate::Verdict::Reject(msg) = r.check_point(&pl.pos) {
            errs.push(format!("place {}: {msg}", pl.name));
        }
    }
    for fr in ferry_routes() {
        // only the island-side waypoint must be inside; routes leave the region
        if let geo_core::validate::Verdict::Reject(msg) = r.check_point(&fr.waypoints[0]) {
            errs.push(format!("ferry {}: {msg}", fr.name));
        }
    }
    errs
}
