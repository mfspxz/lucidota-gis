//! ODIN C2 demo — Scout's 15-city tour route as GeoJSON for tactical mapping.

use crate::{OdinBridge, TacticalNode, NodeType};
use chrono::NaiveDate;

/// Build Scout's Pacific Northwest tour route.
pub fn scout_tour_2026() -> Vec<TacticalNode> {
    vec![
        node("Cortes Island (Home)", 50.05, -124.95, NodeType::HomeBase, None),
        node("Campbell River", 50.02, -125.27, NodeType::TourStop, Some("2026-06-01")),
        node("Courtenay", 49.69, -124.99, NodeType::TourStop, Some("2026-06-03")),
        node("Nanaimo", 49.16, -123.94, NodeType::TourStop, Some("2026-06-05")),
        node("Victoria", 48.43, -123.37, NodeType::TourStop, Some("2026-06-08")),
        node("Vancouver", 49.28, -123.12, NodeType::TourStop, Some("2026-06-11")),
        node("Squamish", 49.70, -123.15, NodeType::TourStop, Some("2026-06-14")),
        node("Whistler", 50.12, -122.95, NodeType::TourStop, Some("2026-06-16")),
        node("Kamloops", 50.67, -120.34, NodeType::TourStop, Some("2026-06-20")),
        node("Revelstoke", 51.00, -118.20, NodeType::TourStop, Some("2026-06-23")),
        node("Nelson", 49.49, -117.29, NodeType::TourStop, Some("2026-06-26")),
        node("Penticton", 49.50, -119.59, NodeType::TourStop, Some("2026-06-29")),
        node("Kelowna", 49.89, -119.49, NodeType::TourStop, Some("2026-07-02")),
        node("Robson Valley Festival", 53.00, -120.00, NodeType::Festival, Some("2026-08-14")),
        node("Home", 50.05, -124.95, NodeType::TourStop, None),
    ]
}

fn node(label: &str, lat: f64, lng: f64, ntype: NodeType, date: Option<&str>) -> TacticalNode {
    TacticalNode {
        id: pt_core::hash_str(label),
        label: label.into(),
        lat, lng,
        node_type: ntype,
        deadline: date.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
        genres: vec!["folk".into(), "folk-punk".into()],
        confidence: 1.0,
        urgency: 0.0,
        source_url: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scout_tour_geojson() {
        let nodes = scout_tour_2026();
        assert_eq!(nodes.len(), 15);

        let odin = OdinBridge::new(None);
        let geojson = odin.build_geojson(&nodes);

        assert_eq!(geojson["type"], "FeatureCollection");
        let features = geojson["features"].as_array().unwrap();
        assert_eq!(features.len(), 15);

        // First node is HomeBase
        let home = &features[0]["properties"];
        assert_eq!(home["icon"], "home_base");

        // Robson Valley Festival has deadline
        let festival = &features[13]["properties"];
        assert_eq!(festival["icon"], "festival");
        assert!(festival["deadline"].as_str().is_some());

        // Route distance should be > 1000 km
        let route = odin.build_route("Scout's Summer 2026 Tour", nodes);
        assert!(route.total_distance_km.unwrap() > 1000.0);
    }

    #[test]
    fn test_geojson_renders_to_string() {
        let nodes = scout_tour_2026();
        let odin = OdinBridge::new(None);
        let geojson = odin.build_geojson(&nodes);
        let json_str = serde_json::to_string(&geojson).unwrap();
        assert!(json_str.contains("FeatureCollection"));
        assert!(json_str.contains("Cortes Island"));
        assert!(json_str.contains("Robson Valley"));
    }
}
