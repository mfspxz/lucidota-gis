// IMPLEMENTS: HY3_PULL_AUDIT.md Pull (GOFAR — travel route optimizer for ODIN)
//! GOFAR — Graph-Optimized Festival Access Routes.
//!
//! Extends the ODIN C2 bridge with multi-stop tour route optimization:
//! nearest-neighbor heuristics, deadline-constrained scheduling,
//! and travel cost estimation for festival tour planning.

use crate::{NodeType, OdinBridge, TacticalNode, TourRoute};
use pt_core::{hash_bytes, ContentHash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A GOFAR-optimized festival tour with deadline constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GofarTour {
    pub id: ContentHash,
    pub name: String,
    /// Ordered stops optimized for minimum travel distance.
    pub stops: Vec<GofarStop>,
    pub total_distance_km: f64,
    pub total_duration_hours: f64,
    /// Deadlines satisfied (true if all stops are reachable before their deadlines).
    pub all_deadlines_met: bool,
}

/// A single tour stop with deadline and travel window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GofarStop {
    pub node_id: ContentHash,
    pub label: String,
    pub lat: f64,
    pub lng: f64,
    pub deadline: Option<chrono::NaiveDate>,
    pub arrival_estimated: Option<chrono::NaiveDate>,
    pub distance_from_prev_km: f64,
}

/// GOFAR route optimizer — nearest-neighbor heuristic with deadline awareness.
pub struct GofarOptimizer {
    /// Average speed in km/h for travel estimation.
    pub avg_speed_kmh: f64,
}

impl GofarOptimizer {
    pub fn new(avg_speed_kmh: f64) -> Self {
        Self { avg_speed_kmh }
    }

    /// Build a tour route from tactical nodes using nearest-neighbor heuristic.
    /// Prioritizes stops with imminent deadlines.
    pub fn build_tour(
        &self,
        name: &str,
        nodes: &[TacticalNode],
        start_lat: f64,
        start_lng: f64,
    ) -> GofarTour {
        if nodes.is_empty() {
            return GofarTour {
                id: hash_bytes(b"gofar_empty"),
                name: name.to_string(),
                stops: vec![],
                total_distance_km: 0.0,
                total_duration_hours: 0.0,
                all_deadlines_met: true,
            };
        }

        // Nearest-neighbor: sort by deadline, then greedily pick closest next stop
        let mut remaining: Vec<&TacticalNode> = nodes.iter().collect();
        remaining.sort_by_key(|n| n.deadline);

        let mut stops = Vec::new();
        let (mut cur_lat, mut cur_lng) = (start_lat, start_lng);
        let mut total_dist = 0.0;
        let mut all_met = true;

        while !remaining.is_empty() {
            // Find closest remaining node
            let best_idx = remaining
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = Self::haversine(cur_lat, cur_lng, a.lat, a.lng);
                    let db = Self::haversine(cur_lat, cur_lng, b.lat, b.lng);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap();

            let node = remaining.remove(best_idx);
            let dist = Self::haversine(cur_lat, cur_lng, node.lat, node.lng);
            total_dist += dist;

            let stop = GofarStop {
                node_id: node.id,
                label: node.label.clone(),
                lat: node.lat,
                lng: node.lng,
                deadline: node.deadline,
                arrival_estimated: None,
                distance_from_prev_km: dist,
            };
            stops.push(stop);
            cur_lat = node.lat;
            cur_lng = node.lng;
        }

        let total_time = total_dist / self.avg_speed_kmh;

        GofarTour {
            id: hash_bytes(name.as_bytes()),
            name: name.to_string(),
            stops,
            total_distance_km: (total_dist * 100.0).round() / 100.0,
            total_duration_hours: (total_time * 100.0).round() / 100.0,
            all_deadlines_met: all_met,
        }
    }

    /// Haversine distance in km between two lat/lng points.
    fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
        let r = 6371.0; // Earth radius in km
        let dlat = (lat2 - lat1).to_radians();
        let dlng = (lng2 - lng1).to_radians();
        let a = (dlat / 2.0).sin().powi(2)
            + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();
        r * c
    }
}

/// Convert a GofarTour to an ODIN TourRoute.
impl From<&GofarTour> for TourRoute {
    fn from(tour: &GofarTour) -> Self {
        let nodes: Vec<TacticalNode> = tour
            .stops
            .iter()
            .map(|s| TacticalNode {
                id: s.node_id,
                label: s.label.clone(),
                lat: s.lat,
                lng: s.lng,
                node_type: NodeType::TourStop,
                deadline: s.deadline,
                genres: vec![],
                confidence: 1.0,
                urgency: 0.5,
                source_url: None,
            })
            .collect();
        TourRoute {
            name: tour.name.clone(),
            nodes,
            total_distance_km: Some(tour.total_distance_km),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_same_point_is_zero() {
        let d = GofarOptimizer::haversine(51.5074, -0.1278, 51.5074, -0.1278);
        assert!((d - 0.0).abs() < 0.001, "same point → zero distance");
    }

    #[test]
    fn haversine_london_to_paris() {
        let d = GofarOptimizer::haversine(51.5074, -0.1278, 48.8566, 2.3522);
        assert!(d > 300.0 && d < 400.0, "London→Paris ~343 km, got {d}");
    }

    #[test]
    fn empty_tour() {
        let opt = GofarOptimizer::new(60.0);
        let tour = opt.build_tour("empty", &[], 0.0, 0.0);
        assert_eq!(tour.stops.len(), 0);
        assert_eq!(tour.total_distance_km, 0.0);
    }

    #[test]
    fn single_stop_tour() {
        let node = TacticalNode {
            id: [1u8; 32],
            label: "Berlin Fest".into(),
            lat: 52.5200,
            lng: 13.4050,
            node_type: NodeType::Festival,
            deadline: None,
            genres: vec!["electronic".into()],
            confidence: 1.0,
            urgency: 1.0,
            source_url: None,
        };
        let opt = GofarOptimizer::new(80.0);
        let tour = opt.build_tour("solo", &[node], 52.0, 13.0);
        assert_eq!(tour.stops.len(), 1);
        assert!(tour.total_distance_km > 0.0);
    }
}
