//! Route calculation — Haversine distance for tour planning.

use crate::TacticalNode;

/// Earth's mean radius in kilometers.
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Haversine distance between two lat/lng points in kilometers.
pub fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    EARTH_RADIUS_KM * c
}

/// Total route distance for a sequence of stops.
pub fn haversine_total(stops: &[TacticalNode]) -> f64 {
    if stops.len() < 2 {
        return 0.0;
    }
    stops
        .windows(2)
        .map(|w| haversine(w[0].lat, w[0].lng, w[1].lat, w[1].lng))
        .sum()
}

/// Placeholder for the Herald trait definition.
/// (Defined in lib.rs to avoid circular deps.)
pub mod herald_stub {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeType, TacticalNode};
    use pt_core::hash_str;

    fn node(label: &str, lat: f64, lng: f64) -> TacticalNode {
        TacticalNode {
            id: hash_str(label),
            label: label.to_string(),
            lat,
            lng,
            node_type: NodeType::TourStop,
            deadline: None,
            genres: vec![],
            confidence: 1.0,
            urgency: 0.0,
            source_url: None,
        }
    }

    #[test]
    fn test_haversine_known_distance() {
        // Vancouver → Seattle: ~190 km
        let d = haversine(49.2827, -123.1207, 47.6062, -122.3321);
        assert!((d - 190.0).abs() < 20.0, "Vancouver→Seattle ≈ 190 km, got {d}");
    }

    #[test]
    fn test_route_total() {
        let stops = vec![
            node("Cortes Island", 50.0, -125.0),
            node("Vancouver", 49.28, -123.12),
            node("Seattle", 47.61, -122.33),
        ];
        let total = haversine_total(&stops);
        assert!(total > 300.0, "total route should be > 300 km");
    }
}
