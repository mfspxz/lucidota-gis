//! open_data — gis-016: public dataset mirrors.
//!
//! A manifest of real, $0, license-clear sources plus a fetcher that shells
//! out to curl (no TLS in std) and logs blake2b-256 digests + timestamps for
//! every mirrored file. Until a mirror fetch succeeds the dataset stays
//! `status: "pending"` — never fake data.

use palontour::{blake2b_256, hex};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Dataset {
    pub id: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    pub format: &'static str,
    pub license: &'static str,
    pub region: &'static str,
}

/// Real, public, $0 sources relevant to Saltspring/Coombs GIS.
pub fn catalog() -> Vec<Dataset> {
    vec![
        Dataset {
            id: "databc-bcgn",
            title: "BC Geographical Names (DataBC WFS)",
            url: "https://openmaps.gov.bc.ca/geo/pub/wfs?service=WFS&version=2.0.0&request=GetFeature&typeName=pub:WHSE_BASEMAPPING.GNS_GEOGRAPHICAL_NAMES_SP&outputFormat=application%2Fjson",
            format: "GML/GeoJSON via WFS",
            license: "Open Government Licence – British Columbia",
            region: "BC",
        },
        Dataset {
            id: "databc-parks",
            title: "BC Parks / protected areas (DataBC WFS)",
            url: "https://openmaps.gov.bc.ca/geo/pub/wfs?service=WFS&request=GetFeature&typeName=pub:WHSE_TANTALIS.TA_PARK_ECORES_PA_SVW",
            format: "GML via WFS",
            license: "Open Government Licence – British Columbia",
            region: "BC",
        },
        Dataset {
            id: "databc-parcels-fabric",
            title: "ParcelMap BC (DataBC WFS, parcel fabric)",
            url: "https://openmaps.gov.bc.ca/geo/pub/wfs?service=WFS&request=GetFeature&typeName=pub:WHSE_CADASTRE.PMBC_PARCEL_FABRIC_POLY_SVW",
            format: "GML via WFS",
            license: "Open Government Licence – British Columbia",
            region: "BC",
        },
        Dataset {
            id: "osm-bc-geofabrik",
            title: "OpenStreetMap British Columbia extract (Geofabrik)",
            url: "https://download.geofabrik.de/north-america/canada/british-columbia-latest.osm.pbf",
            format: "OSM PBF",
            license: "ODbL 1.0",
            region: "BC",
        },
        Dataset {
            id: "nrcan-toporama",
            title: "NRCan Toporama WMS (topographic basemap)",
            url: "https://maps.geogratis.gc.ca/wms/toporama_en?service=WMS&request=GetCapabilities",
            format: "WMS",
            license: "Open Government Licence – Canada",
            region: "Canada",
        },
    ]
}

#[derive(Debug)]
pub struct MirrorLog {
    pub entries: Vec<MirrorEntry>,
}

#[derive(Debug, Clone)]
pub struct MirrorEntry {
    pub dataset_id: String,
    pub url: String,
    pub fetched_unix: i64,
    pub bytes: u64,
    pub digest: String,
    pub path: String,
    pub ok: bool,
    pub note: String,
}

/// Mirror one dataset with curl. file:// URLs work too (used by tests).
/// Never panics on network failure: records ok=false and moves on.
pub fn mirror(ds: &Dataset, out_dir: &str) -> MirrorEntry {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let dest = format!("{out_dir}/{}.bin", ds.id);
    let mut entry = MirrorEntry {
        dataset_id: ds.id.into(), url: ds.url.into(), fetched_unix: now,
        bytes: 0, digest: String::new(), path: dest.clone(), ok: false,
        note: String::new(),
    };
    let _ = std::fs::create_dir_all(out_dir);
    let res = std::process::Command::new("curl")
        .args(["-sS", "-L", "--max-time", "60", "--retry", "1", "-o", &dest, ds.url])
        .output();
    match res {
        Ok(o) if o.status.success() => {
            match std::fs::read(&dest) {
                Ok(bytes) => {
                    entry.bytes = bytes.len() as u64;
                    entry.digest = hex(&blake2b_256(&bytes));
                    entry.ok = entry.bytes > 0;
                    entry.note = if entry.ok { "mirrored".into() } else { "empty body".into() };
                }
                Err(e) => entry.note = format!("read failed: {e}"),
            }
        }
        Ok(o) => entry.note = format!("curl exit {}: {}", o.status, String::from_utf8_lossy(&o.stderr).chars().take(200).collect::<String>()),
        Err(e) => entry.note = format!("curl spawn failed: {e}"),
    }
    entry
}

pub fn manifest_json(catalog: &[Dataset], log: &[MirrorEntry]) -> String {
    let mut ds = String::new();
    for (i, d) in catalog.iter().enumerate() {
        if i > 0 { ds.push(','); }
        let status = log.iter().find(|e| e.dataset_id == d.id)
            .map(|e| if e.ok { "mirrored" } else { "pending" }).unwrap_or("pending");
        ds.push_str(&format!(
            "{{\"id\":\"{}\",\"title\":\"{}\",\"format\":\"{}\",\"license\":\"{}\",\"region\":\"{}\",\"status\":\"{}\"}}",
            d.id, d.title, d.format, d.license, d.region, status
        ));
    }
    let mut lg = String::new();
    for (i, e) in log.iter().enumerate() {
        if i > 0 { lg.push(','); }
        lg.push_str(&format!(
            "{{\"dataset\":\"{}\",\"fetched\":{},\"bytes\":{},\"digest\":\"{}\",\"ok\":{},\"note\":\"{}\"}}",
            e.dataset_id, e.fetched_unix, e.bytes, e.digest, e.ok,
            e.note.replace('"', "'").chars().take(160).collect::<String>()
        ));
    }
    format!("{{\"datasets\":[{ds}],\"mirror_log\":[{lg}]}}")
}

pub fn write_manifest(path: &str, catalog: &[Dataset], log: &[MirrorEntry]) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, manifest_json(catalog, log))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_real_and_clear() {
        let c = catalog();
        assert!(c.len() >= 5);
        assert!(c.iter().all(|d| d.url.starts_with("https://")));
        assert!(c.iter().all(|d| d.license.contains("Open Government") || d.license.contains("ODbL")));
        // parcel fabric source exists so gis-004's synthetic fabric has a real upgrade path
        assert!(c.iter().any(|d| d.id.contains("parcel")));
    }

    #[test]
    fn file_url_mirror_roundtrip() {
        let dir = "/tmp/open_data_fixture";
        std::fs::create_dir_all(dir).unwrap();
        let fixture = format!("{dir}/fixture.geojson");
        std::fs::write(&fixture, "{\"type\":\"FeatureCollection\",\"features\":[]}").unwrap();
        let ds = Dataset {
            id: "test-fixture", title: "t", format: "GeoJSON", license: "ODbL 1.0", region: "test",
            url: Box::leak(format!("file://{fixture}").into_boxed_str()),
        };
        let e = mirror(&ds, "/tmp/open_data_mirror");
        assert!(e.ok, "note={}", e.note);
        assert_eq!(e.digest.len(), 64);
        let m = manifest_json(&[ds], std::slice::from_ref(&e));
        assert!(m.contains("\"status\":\"mirrored\""));
    }

    #[test]
    fn network_failure_is_recorded_not_fatal() {
        let ds = Dataset {
            id: "test-dead", title: "t", format: "x", license: "ODbL 1.0", region: "test",
            url: "https://127.0.0.1:1/definitely-not-here",
        };
        let e = mirror(&ds, "/tmp/open_data_mirror");
        assert!(!e.ok);
        assert!(!e.note.is_empty());
        let m = manifest_json(&[ds], std::slice::from_ref(&e));
        assert!(m.contains("\"status\":\"pending\""));
    }
}
