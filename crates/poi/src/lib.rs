//! poi — gis-011: points-of-interest database.
//!
//! In-memory DB with JSON-file persistence. POIs are user-addable; seeds come
//! from the saltspring/coombs canonical datasets. Manual JSON (no serde).

use geo_core::coord::LatLon;

#[derive(Debug, Clone)]
pub struct Poi {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub pos: LatLon,
    pub notes: String,
    pub user_added: bool,
    pub created_unix: i64,
}

#[derive(Debug, Default)]
pub struct PoiDb {
    pois: Vec<Poi>,
    next_id: u64,
}

impl PoiDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: &str, kind: &str, lat: f64, lon: f64, notes: &str, user_added: bool) -> Result<String, String> {
        let pos = LatLon::new(lat, lon).ok_or_else(|| format!("bad coords {lat},{lon}"))?;
        self.next_id += 1;
        let id = format!("poi-{:06}", self.next_id);
        self.pois.push(Poi {
            id: id.clone(), name: name.into(), kind: kind.into(), pos,
            notes: notes.into(), user_added,
            created_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
        });
        Ok(id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.pois.len();
        self.pois.retain(|p| p.id != id);
        self.pois.len() < n
    }

    /// Full snapshot (cloned) for graph builders and exporters.
    pub fn snapshot(&self) -> Vec<Poi> {
        self.pois.clone()
    }

    pub fn get(&self, id: &str) -> Option<&Poi> {
        self.pois.iter().find(|p| p.id == id)
    }

    pub fn len(&self) -> usize {
        self.pois.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pois.is_empty()
    }

    /// All POIs within radius_m of center, nearest first.
    pub fn near(&self, center: &LatLon, radius_m: f64) -> Vec<(&Poi, f64)> {
        let mut out: Vec<(&Poi, f64)> = self.pois.iter()
            .map(|p| (p, center.distance_m(&p.pos)))
            .filter(|(_, d)| *d <= radius_m)
            .collect();
        out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        out
    }

    pub fn by_kind(&self, kind: &str) -> Vec<&Poi> {
        self.pois.iter().filter(|p| p.kind == kind).collect()
    }

    pub fn user_added(&self) -> Vec<&Poi> {
        self.pois.iter().filter(|p| p.user_added).collect()
    }

    pub fn seed_saltspring(&mut self) {
        for pl in saltspring::places() {
            let _ = self.add(pl.name, pl.kind, pl.pos.lat, pl.pos.lon, pl.note, false);
        }
    }

    pub fn seed_coombs(&mut self) {
        for pl in coombs::places() {
            let _ = self.add(pl.name, pl.kind, pl.pos.lat, pl.pos.lon, pl.note, false);
        }
    }

    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let mut j = String::from("{\"pois\":[");
        for (i, p) in self.pois.iter().enumerate() {
            if i > 0 { j.push(','); }
            j.push_str(&format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"kind\":\"{}\",\"lat\":{:.6},\"lon\":{:.6},\"notes\":\"{}\",\"user_added\":{},\"created\":{}}}",
                p.id, Self::esc(&p.name), Self::esc(&p.kind), p.pos.lat, p.pos.lon,
                Self::esc(&p.notes), p.user_added, p.created_unix
            ));
        }
        j.push_str("]}");
        std::fs::write(path, j)
    }

    pub fn load(path: &str) -> std::io::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        let mut db = PoiDb::new();
        // minimal parser: split on {"id":" boundaries
        for chunk in s.split("{\"id\":\"").skip(1) {
            let get = |key: &str| -> String {
                chunk.split(&format!("\"{key}\":\"")).nth(1)
                    .and_then(|v| v.split('"').next()).unwrap_or("").to_string()
            };
            let getn = |key: &str| -> f64 {
                chunk.split(&format!("\"{key}\":")).nth(1)
                    .and_then(|v| v.split(|c| c == ',' || c == '}').next())
                    .and_then(|v| v.parse().ok()).unwrap_or(0.0)
            };
            let id = chunk.split('"').next().unwrap_or("").to_string();
            let lat = getn("lat");
            let lon = getn("lon");
            if let Some(pos) = LatLon::new(lat, lon) {
                let n: u64 = id.strip_prefix("poi-").and_then(|x| x.parse().ok()).unwrap_or(0);
                db.next_id = db.next_id.max(n);
                db.pois.push(Poi {
                    id,
                    name: get("name"), kind: get("kind"), pos,
                    notes: get("notes"),
                    user_added: chunk.contains("\"user_added\":true"),
                    created_unix: getn("created") as i64,
                });
            }
        }
        Ok(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_and_search() {
        let mut db = PoiDb::new();
        db.seed_saltspring();
        db.seed_coombs();
        assert!(db.len() >= 20);
        let ganges = LatLon::new_unchecked(48.8541, -123.4994);
        let near = db.near(&ganges, 3_000.0);
        assert!(!near.is_empty());
        assert_eq!(near[0].0.name, "Ganges");
        assert!(!db.by_kind("ferry").is_empty());
    }

    #[test]
    fn user_add_and_remove() {
        let mut db = PoiDb::new();
        let id = db.add("Secret swim spot", "swim", 48.83, -123.45, "locals only", true).unwrap();
        assert_eq!(db.user_added().len(), 1);
        assert!(db.add("bad", "x", 999.0, 0.0, "", true).is_err());
        assert!(db.remove(&id));
        assert!(!db.remove(&id));
    }

    #[test]
    fn persistence_roundtrip() {
        let mut db = PoiDb::new();
        db.seed_saltspring();
        db.add("My dock", "dock", 48.90, -123.50, "mine", true).unwrap();
        let p = "/tmp/poi_db_test.json";
        db.save(p).unwrap();
        let back = PoiDb::load(p).unwrap();
        assert_eq!(back.len(), db.len());
        assert_eq!(back.user_added().len(), 1);
        assert_eq!(back.user_added()[0].name, "My dock");
        // id counter survives reload
        let mut back = back;
        let id2 = back.add("Another", "x", 48.85, -123.49, "", true).unwrap();
        assert!(!back.get(&id2).is_none());
        std::fs::remove_file(p).ok();
    }
}
