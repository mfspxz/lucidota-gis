//! odin-viewer — the ODIN C2 tactical map launcher (shell over pt-bridge-odin).
//!
//! Builds tactical nodes from a live Herald-style digest feed and/or the
//! demo tour, serializes them to GeoJSON (RFC 7946), and serves an interactive
//! Leaflet map on a local HTTP port. Single process, in-memory, restartable.
//!
//! Feed source: reads a JSON array of feed entries from the path set in
//! `ODIN_FEED_PATH` (defaults to `/tmp/odin_feed.json`).  When the file is
//! absent, falls back to the static `scout_tour_2026()` demo.  The feed
//! format is designed so that `pt-bridge-matrix` (or any Herald consumer)
//! can write it — one JSON object per event, with lat/lng for the map.

use pt_bridge_odin::{NodeType, OdinBridge, TacticalNode, TourRoute};
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

const ADDR: &str = "127.0.0.1:8088";

/// A single entry in the ODIN feed file — maps directly to a TacticalNode.
#[derive(Debug, Clone, Deserialize)]
struct FeedEntry {
    label: String,
    lat: f64,
    lng: f64,
    #[serde(default)]
    node_type: FeedNodeType,
    deadline: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default = "default_confidence")]
    confidence: f32,
    #[serde(default)]
    urgency: f32,
    source_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum FeedNodeType {
    #[default]
    Venue,
    Festival,
    ApplicationWindow,
    TourStop,
    HomeBase,
}

fn default_confidence() -> f32 {
    0.5
}

impl From<FeedEntry> for TacticalNode {
    fn from(e: FeedEntry) -> Self {
        let node_type = match e.node_type {
            FeedNodeType::Venue => NodeType::Venue,
            FeedNodeType::Festival => NodeType::Festival,
            FeedNodeType::ApplicationWindow => NodeType::ApplicationWindow,
            FeedNodeType::TourStop => NodeType::TourStop,
            FeedNodeType::HomeBase => NodeType::HomeBase,
        };
        let deadline = e.deadline.and_then(|d| {
            chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .or_else(|_| chrono::NaiveDate::parse_from_str(&d, "%Y/%m/%d"))
                .ok()
        });
        TacticalNode {
            id: pt_core::hash_str(&e.label),
            label: e.label,
            lat: e.lat,
            lng: e.lng,
            node_type,
            deadline,
            genres: e.genres,
            confidence: e.confidence,
            urgency: e.urgency,
            source_url: e.source_url,
        }
    }
}

/// Try to load the ODIN feed file. Returns `None` when the file is absent
/// (demo tour will be used as fallback), or an error when the file exists
/// but is unreadable / unparseable.
fn load_feed(path: &std::path::Path) -> anyhow::Result<Option<Vec<TacticalNode>>> {
    if !path.exists() {
        tracing::info!(path = %path.display(), "ODIN feed file not found — falling back to demo tour");
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        tracing::warn!(path = %path.display(), "ODIN feed file is empty");
        return Ok(None);
    }
    let entries: Vec<FeedEntry> = serde_json::from_str(&raw)?;
    let nodes: Vec<TacticalNode> = entries.into_iter().map(TacticalNode::from).collect();
    tracing::info!(path = %path.display(), count = nodes.len(), "ODIN feed loaded");
    Ok(Some(nodes))
}

/// Resolve the feed file path: `ODIN_FEED_PATH` env var, else `/tmp/odin_feed.json`.
fn feed_path() -> PathBuf {
    std::env::var("ODIN_FEED_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/odin_feed.json"))
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "odin_viewer=info".into()),
        )
        .init();

    let feed_path = feed_path();

    // --- Build the tactical picture ---
    // Load live feed if available; always keep demo tour as fallback / floor.
    let demo_nodes = pt_bridge_odin::tour_demo::scout_tour_2026();

    let feed_nodes = load_feed(&feed_path)?;
    let (nodes, feed_source) = if let Some(live) = feed_nodes {
        tracing::info!(
            demo = demo_nodes.len(),
            live = live.len(),
            "merging live feed into demo tour"
        );
        let mut combined = live;
        combined.extend(demo_nodes);
        (combined, "live feed + demo tour")
    } else {
        (demo_nodes, "demo tour")
    };

    let odin = OdinBridge::new(None);
    let geojson = odin.build_geojson(&nodes);
    let geojson_str = serde_json::to_string_pretty(&geojson)?;

    // GOFAR-optimized route as a sanity signal (logged, also embedded as a line note).
    let gofar = pt_bridge_odin::gofar::GofarOptimizer::new(80.0)
        .build_tour("Scout GOFAR PNW", &nodes, 50.05, -124.95);
    let _route: TourRoute = TourRoute::from(&gofar);
    tracing::info!(
        nodes = nodes.len(),
        source = feed_source,
        geojson_features = geojson["features"].as_array().map(|f| f.len()).unwrap_or(0),
        gofar_km = gofar.total_distance_km,
        "ODIN tactical picture built"
    );

    let html = render_html();

    // --- Serve it (hand-rolled minimal HTTP/1.1; no web framework dep) ---
    let listener = TcpListener::bind(ADDR)?;
    tracing::info!("ODIN C2 tactical map LIVE at http://{}/", ADDR);
    println!("ODIN C2 tactical map LIVE at http://{}/", ADDR);
    println!("  source        : {}", feed_source);
    println!("  feed path     : {}", feed_path.display());
    println!("  geojson       : http://{}/odin.geojson", ADDR);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let html = html.clone();
                let geo = geojson_str.clone();
                thread::spawn(move || handle(s, &html, &geo));
            }
            Err(e) => tracing::warn!(error = %e, "accept failed"),
        }
    }
    Ok(())
}

fn handle(mut stream: std::net::TcpStream, html: &str, geojson: &str) {
    let mut buf = [0u8; 2048];
    if stream.read(&mut buf).is_err() {
        return;
    }
    let req = String::from_utf8_lossy(&buf);
    let path = req
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/");

    let (status, ctype, body): (&str, &str, &str) = match path {
        "/odin.geojson" => ("200 OK", "application/json; charset=utf-8", geojson),
        _ => ("200 OK", "text/html; charset=utf-8", html),
    };

    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        ctype,
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn render_html() -> String {
    r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>ODIN C2 — Scout Tactical Map</title>
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css"
      integrity="sha256-p4NxAoJBhIIN+hmNHrzRCf9tD/miZyoHS5obTRR9BMY=" crossorigin=""/>
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"
        integrity="sha256-20nQCchB9co0qIjJZRGuk2/Z9VM+kNiyxNV1lvTlZBo=" crossorigin=""></script>
<style>html,body{margin:0;height:100%}#map{height:100vh;width:100%}</style>
</head>
<body>
<div id="map"></div>
<script>
const m = L.map('map').setView([50.0, -123.5], 6);
L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
  maxZoom: 19, attribution: '&copy; OpenStreetMap'
}).addTo(m);
fetch('odin.geojson')
  .then(r => r.json())
  .then(g => {
    L.geoJSON(g, {
      onEachFeature: (f, ly) => {
        const p = f.properties || {};
        ly.bindPopup(
          '<b>' + (p.label || '?') + '</b><br>' +
          'type: ' + (p.icon || '?') +
          (p.deadline ? '<br>deadline: ' + p.deadline : '') +
          (p.days_until_deadline != null ? ' (' + p.days_until_deadline + 'd)' : '') +
          '<br>conf: ' + (p.confidence != null ? p.confidence : '?')
        );
      },
      pointToLayer: (f, latlng) => {
        const p = f.properties || {};
        const color = p.urgency_color || '#3388ff';
        return L.circleMarker(latlng, {
          radius: 8, color: '#222', weight: 1,
          fillColor: color, fillOpacity: 0.9
        });
      }
    }).addTo(m);
    const b = L.geoJSON(g).getBounds();
    if (b.isValid()) m.fitBounds(b);
  })
  .catch(e => {
    document.getElementById('map').innerHTML =
      '<p style="padding:1em;font-family:monospace">odin.geojson failed to load: ' + e + '</p>';
  });
</script>
</body>
</html>"#
        .to_string()
}
