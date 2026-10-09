//! GeoJSON export — RFC 7946 FeatureCollection for C2 map systems.
//!
//! Every tactical node becomes a GeoJSON Feature with properties
//! that any C2 tool can render: icon, urgency color, deadline countdown.

use crate::TacticalNode;
use serde_json::{json, Value};

/// Build a GeoJSON FeatureCollection from tactical nodes.
pub fn feature_collection(nodes: &[TacticalNode]) -> Value {
    let features: Vec<Value> = nodes.iter().map(|n| feature(n)).collect();
    json!({
        "type": "FeatureCollection",
        "features": features
    })
}

/// Build a single GeoJSON Feature from a tactical node.
fn feature(node: &TacticalNode) -> Value {
    let icon = match node.node_type {
        crate::NodeType::Venue => "venue",
        crate::NodeType::Festival => "festival",
        crate::NodeType::ApplicationWindow => "deadline",
        crate::NodeType::TourStop => "tour_stop",
        crate::NodeType::HomeBase => "home_base",
    };

    let urgency_color = if node.urgency > 7.0 {
        "#ff0000" // red: critical
    } else if node.urgency > 3.0 {
        "#ffaa00" // orange: soon
    } else {
        "#00aa00" // green: comfortable
    };

    let mut properties = json!({
        "label": node.label,
        "icon": icon,
        "urgency": node.urgency,
        "urgency_color": urgency_color,
        "confidence": node.confidence,
        "genres": node.genres,
    });

    if let Some(deadline) = node.deadline {
        properties["deadline"] = json!(deadline.to_string());
        let days_left = (deadline - chrono::Utc::now().date_naive()).num_days();
        properties["days_until_deadline"] = json!(days_left);
    }

    if let Some(ref url) = node.source_url {
        properties["source_url"] = json!(url);
    }

    json!({
        "type": "Feature",
        "geometry": {
            "type": "Point",
            "coordinates": [node.lng, node.lat]
        },
        "properties": properties
    })
}
