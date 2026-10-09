//! saltspring_data — gis-002: Saltspring data pipeline.
//!
//! Ingest -> change-detect -> versioned snapshots. Sources implement `Source`;
//! the pipeline fingerprints every record (FNV-1a, documented non-crypto —
//! tamper-evidence lives in the palontour CAS) and only promotes new/changed
//! records. Snapshots land in data/v{n}/ with a manifest.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct IngestRecord {
    pub source: String,
    pub kind: String,
    pub id: String,
    pub payload: String, // GeoJSON
}

impl IngestRecord {
    /// Change fingerprint (FNV-1a 64). Non-crypto by design; see module docs.
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in self.payload.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

pub trait Source {
    fn name(&self) -> &str;
    fn fetch(&self) -> std::io::Result<Vec<IngestRecord>>;
}

/// Built-in source: regenerates layers from the saltspring canonical dataset.
pub struct CanonicalSource;
impl Source for CanonicalSource {
    fn name(&self) -> &str {
        "saltspring-canonical"
    }
    fn fetch(&self) -> std::io::Result<Vec<IngestRecord>> {
        Ok(saltspring::layers_geojson()
            .into_iter()
            .map(|(name, gj)| IngestRecord {
                source: self.name().into(),
                kind: "geojson-layer".into(),
                id: name,
                payload: gj,
            })
            .collect())
    }
}

/// Directory source: picks up *.geojson drops from a directory (the "update" path).
pub struct DirSource {
    pub dir: String,
}
impl Source for DirSource {
    fn name(&self) -> &str {
        "dir-drop"
    }
    fn fetch(&self) -> std::io::Result<Vec<IngestRecord>> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(_) => return Ok(out),
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("geojson") {
                let id = p.file_stem().and_then(|x| x.to_str()).unwrap_or("?").to_string();
                out.push(IngestRecord {
                    source: self.name().into(),
                    kind: "geojson-layer".into(),
                    id,
                    payload: std::fs::read_to_string(&p)?,
                });
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Default)]
pub struct UpdateReport {
    pub added: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub snapshot_version: u64,
}

/// Run the pipeline: fetch from all sources, diff against last snapshot state,
/// write a new versioned snapshot only if something changed.
pub fn run_pipeline(sources: &[Box<dyn Source>], data_dir: &str) -> std::io::Result<UpdateReport> {
    std::fs::create_dir_all(data_dir)?;
    let state_path = format!("{data_dir}/state.json");
    let mut prev: HashMap<String, u64> = HashMap::new();
    if Path::new(&state_path).exists() {
        let s = std::fs::read_to_string(&state_path)?;
        for pair in s.trim_matches(|c| c == '{' || c == '}').split(',') {
            // keys are "source:id" (contain ':'), so split on the LAST colon
            if let Some((k, v)) = pair.rsplit_once(':') {
                if let Ok(h) = v.trim().parse::<u64>() {
                    prev.insert(k.trim().trim_matches('"').to_string(), h);
                }
            }
        }
    }
    let mut report = UpdateReport::default();
    let mut cur: HashMap<String, u64> = HashMap::new();
    let mut changed_payloads: Vec<&IngestRecord> = Vec::new();
    let mut all: Vec<IngestRecord> = Vec::new();
    for src in sources {
        all.extend(src.fetch()?);
    }
    for rec in &all {
        let key = format!("{}:{}", rec.source, rec.id);
        let fp = rec.fingerprint();
        cur.insert(key.clone(), fp);
        match prev.get(&key) {
            None => {
                report.added += 1;
                changed_payloads.push(rec);
            }
            Some(&old) if old != fp => {
                report.changed += 1;
                changed_payloads.push(rec);
            }
            _ => report.unchanged += 1,
        }
    }
    // version = max existing + 1 if anything changed, else current max
    let mut ver: u64 = 0;
    if let Ok(rd) = std::fs::read_dir(data_dir) {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str().and_then(|s| s.strip_prefix('v')) {
                if let Ok(v) = n.parse::<u64>() {
                    ver = ver.max(v);
                }
            }
        }
    }
    if report.added + report.changed > 0 {
        ver += 1;
        let snap = format!("{data_dir}/v{ver}");
        std::fs::create_dir_all(&snap)?;
        for rec in changed_payloads {
            std::fs::write(format!("{snap}/{}.geojson", rec.id), &rec.payload)?;
        }
        let manifest = format!(
            "{{\"version\":{ver},\"added\":{},\"changed\":{},\"records\":{}}}",
            report.added, report.changed, all.len()
        );
        std::fs::write(format!("{snap}/manifest.json"), manifest)?;
    }
    report.snapshot_version = ver;
    // persist state
    let mut sj = String::from("{");
    let mut first = true;
    let mut keys: Vec<_> = cur.keys().collect();
    keys.sort();
    for k in keys {
        if !first { sj.push(','); }
        first = false;
        sj.push_str(&format!("\"{}\":{}", k, cur[k]));
    }
    sj.push('}');
    std::fs::write(&state_path, sj)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir(tag: &str) -> String {
        let d = format!("/tmp/saltspring_data_{tag}");
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn first_run_adds_all() {
        let d = data_dir("first");
        let srcs: Vec<Box<dyn Source>> = vec![Box::new(CanonicalSource)];
        let r = run_pipeline(&srcs, &d).unwrap();
        assert_eq!(r.snapshot_version, 1);
        assert!(r.added >= 3, "added={}", r.added);
        assert!(Path::new(&format!("{d}/v1/manifest.json")).exists());
    }

    #[test]
    fn second_run_no_change_no_snapshot() {
        let d = data_dir("second");
        let srcs: Vec<Box<dyn Source>> = vec![Box::new(CanonicalSource)];
        run_pipeline(&srcs, &d).unwrap();
        let r = run_pipeline(&srcs, &d).unwrap();
        assert_eq!(r.unchanged, r.added + r.changed + r.unchanged);
        assert_eq!(r.snapshot_version, 1, "no new snapshot when nothing changed");
    }

    #[test]
    fn dir_drop_detected_as_change() {
        let d = data_dir("dir");
        let drop = format!("{d}/drops");
        std::fs::create_dir_all(&drop).unwrap();
        let srcs: Vec<Box<dyn Source>> = vec![Box::new(CanonicalSource), Box::new(DirSource { dir: drop.clone() })];
        run_pipeline(&srcs, &d).unwrap();
        std::fs::write(format!("{drop}/ferry_update.geojson"),
            "{\"type\":\"FeatureCollection\",\"features\":[]}").unwrap();
        let r = run_pipeline(&srcs, &d).unwrap();
        assert_eq!(r.added, 1, "report={r:?}");
        assert_eq!(r.snapshot_version, 2);
    }
}
