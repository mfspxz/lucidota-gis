//! Static site generator: Leaflet map + GeoJSON layers. The generated site is
//! the "live site" artifact — deploy it on any static host (cutecity infra).

use crate::data;
use std::io::Write;

const LEAFLET_CSS: &str = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.css";
const LEAFLET_JS: &str = "https://unpkg.com/leaflet@1.9.4/dist/leaflet.js";

pub fn render_site(out_dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(format!("{out_dir}/data"))?;
    for (name, gj) in data::layers_geojson() {
        std::fs::write(format!("{out_dir}/data/{name}.geojson"), gj)?;
    }

    let mut markers = String::new();
    for pl in data::places() {
        markers.push_str(&format!(
            "L.marker([{:.4},{:.4}]).addTo(map).bindPopup(\"<b>{}</b><br/>{} — {}\");\n",
            pl.pos.lat, pl.pos.lon, pl.name, pl.kind, pl.note.replace('"', "")
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"/>
<title>Saltspring Slasher GIS</title>
<link rel="stylesheet" href="{LEAFLET_CSS}"/>
<style>html,body,#map{{height:100%;margin:0;background:#0a0a0f}}
.hdr{{position:absolute;z-index:999;top:10px;left:50px;color:#ff4fd8;font:700 18px monospace;
background:rgba(10,10,15,.8);padding:6px 12px;border:1px solid #ff4fd8}}</style></head>
<body><div id="map"></div><div class="hdr">SALTSRING SLASHER GIS</div>
<script src="{LEAFLET_JS}"></script>
<script>
var map = L.map('map').setView([48.86,-123.49], 11);
L.tileLayer('https://tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png',
  {{maxZoom:19, attribution:'&copy; OpenStreetMap contributors'}}).addTo(map);
{markers}
["ferry_routes","roads"].forEach(function(layer){{
  fetch('data/'+layer+'.geojson').then(r=>r.json()).then(j=>{{
    L.geoJSON(j,{{style:{{color:'#ff4fd8',weight:2}}}}).addTo(map);
  }});
}});
</script></body></html>"#
    );
    let mut f = std::fs::File::create(format!("{out_dir}/index.html"))?;
    f.write_all(html.as_bytes())?;
    Ok(())
}
