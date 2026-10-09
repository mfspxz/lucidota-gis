//! pt-bridge-odin — ODIN C2 tactical map integration.
//!
//! Pushes venue coordinates, application windows, and travel routes
//! as tactical GeoJSON nodes to any C2 mapping system.
//!
//! The Herald trait is shared with pt-bridge-matrix — both consume
//! the same diff/digest feed. ODIN gets the map layer.

pub mod geojson;
pub mod gofar;
pub mod herald;
pub mod route;
pub mod tour_demo;

use pt_core::ContentHash;
use serde::{Deserialize, Serialize};

/// A tactical node on the C2 map. One node per venue/festival/event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalNode {
    /// Unique node ID (event hash).
    pub id: ContentHash,
    /// Display label.
    pub label: String,
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
    /// Node type determines the icon on the map.
    pub node_type: NodeType,
    /// Application deadline (None = no deadline / rolling).
    pub deadline: Option<chrono::NaiveDate>,
    /// Genre tags for filtering.
    pub genres: Vec<String>,
    /// Epistemic confidence: 0.0 = unverified, 1.0 = proven.
    pub confidence: f32,
    /// How urgent this node is (days until deadline, or distance).
    pub urgency: f32,
    /// URL to the source page.
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Venue,
    Festival,
    ApplicationWindow,
    TourStop,
    HomeBase,
}

/// A tour route: ordered sequence of tactical nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourRoute {
    pub name: String,
    pub nodes: Vec<TacticalNode>,
    pub total_distance_km: Option<f64>,
}

/// Shared Herald trait — implemented by both ODIN and Matrix bridges.
/// Palantour pushes diffs and digests; bridges translate to their format.
#[async_trait::async_trait]
pub trait Herald {
    /// Push a single entity diff (event changed, deadline moved, etc.).
    async fn announce_diff(&self, event_id: ContentHash, diff: &str) -> anyhow::Result<()>;

    /// Push a morning digest (ranked feed, deadline alerts).
    async fn announce_digest(&self, digest: &str) -> anyhow::Result<()>;
}

/// The ODIN bridge: converts tactical nodes to GeoJSON and pushes to C2.
pub struct OdinBridge {
    output_path: Option<std::path::PathBuf>,
}

impl OdinBridge {
    pub fn new(output_path: Option<std::path::PathBuf>) -> Self {
        Self { output_path }
    }

    /// Build a GeoJSON FeatureCollection from tactical nodes.
    pub fn build_geojson(&self, nodes: &[TacticalNode]) -> serde_json::Value {
        geojson::feature_collection(nodes)
    }

    /// Export nodes to a GeoJSON file for C2 import.
    pub fn export_geojson(&self, nodes: &[TacticalNode]) -> anyhow::Result<()> {
        let geojson = self.build_geojson(nodes);
        let json_str = serde_json::to_string_pretty(&geojson)?;
        if let Some(ref path) = self.output_path {
            std::fs::write(path, &json_str)?;
            tracing::info!(path = %path.display(), nodes = nodes.len(), "GeoJSON exported for ODIN C2");
        }
        Ok(())
    }

    /// Build a tour route from a sequence of venues.
    pub fn build_route(&self, name: &str, stops: Vec<TacticalNode>) -> TourRoute {
        let distance = route::haversine_total(&stops);
        TourRoute {
            name: name.to_string(),
            nodes: stops,
            total_distance_km: Some(distance),
        }
    }
}

#[async_trait::async_trait]
impl Herald for OdinBridge {
    async fn announce_diff(&self, _event_id: ContentHash, diff: &str) -> anyhow::Result<()> {
        tracing::info!(diff = %diff, "ODIN C2 diff");
        Ok(())
    }

    async fn announce_digest(&self, digest: &str) -> anyhow::Result<()> {
        tracing::info!(digest = %digest, "ODIN C2 digest");
        Ok(())
    }
}
