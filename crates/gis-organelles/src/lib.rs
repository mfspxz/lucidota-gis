//! gis as organelles (Tier A native migration).
//!
//! Three organelles on the code's real seams:
//! - `gis-geo` (`ge`): pure geodesy over geo_core — haversine, bearing,
//!   destination, tile math, projections. ReadOnly/Proven.
//! - `gis-poi` (`gi`): the POI database behind a Mutex — add/remove/get,
//!   radius + kind queries, seed datasets, JSON save/load. Write-gated.
//! - `gis-route` (`gj`): the pedicab routing graph behind a Mutex —
//!   add_node/add_leg, Dijkstra shortest under pedicab|direct profile.
//!   Write-gated for mutation, pure for queries.

use geo_core::coord::{LatLon, WebMercator};
use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use organelle_manifest::{
    Authority, Contract, MutationClass, OrganelleManifest, ReceiptPolicy, Transport, Valence,
    MANIFEST_SCHEMA_VERSION, RECEIPT_ALGORITHM,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn str_in(v: &Value, key: &str) -> Result<String, OrganelleError> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| OrganelleError::failed(format!("missing string field {key:?}")))
}

fn num_in(v: &Value, key: &str) -> Result<f64, OrganelleError> {
    v.get(key)
        .and_then(|x| x.as_f64())
        .ok_or_else(|| OrganelleError::failed(format!("missing number field {key:?}")))
}

fn int_in(v: &Value, key: &str) -> Result<i64, OrganelleError> {
    v.get(key)
        .and_then(|x| x.as_i64())
        .ok_or_else(|| OrganelleError::failed(format!("missing integer field {key:?}")))
}

fn failed(s: String) -> OrganelleError {
    OrganelleError::failed(s)
}

fn contract(op: &str, description: &str, schema: Value) -> Contract {
    Contract { op: op.to_string(), description: description.to_string(), input_schema: schema, mutation_class: None, required_capability: None }
}

fn contract_gated(op: &str, description: &str, schema: Value, cap: &str) -> Contract {
    Contract { op: op.to_string(), description: description.to_string(), input_schema: schema, mutation_class: Some(MutationClass::Write), required_capability: Some(cap.to_string()) }
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn latlon_in(v: &Value) -> Result<LatLon, OrganelleError> {
    LatLon::new(num_in(v, "lat")?, num_in(v, "lon")?)
        .ok_or_else(|| failed("lat/lon out of range".to_string()))
}

fn latlon_json(p: &LatLon) -> Value {
    json!({ "lat": p.lat, "lon": p.lon })
}

fn manifest_base(name: &str, codon: &str, description: &str, contracts: Vec<Contract>, valence: Valence) -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: name.to_string(),
        codon: codon.to_string(),
        version: "0.1.0".to_string(),
        description: description.to_string(),
        contracts,
        authority: Authority {
            mutation_class: MutationClass::ReadOnly,
            declared_valence: valence,
            required_capability: None,
            granted_capabilities: vec![],
        },
        transports: Transport::all().to_vec(),
        receipt_policy: ReceiptPolicy { content_addressed: true, algorithm: RECEIPT_ALGORITHM.to_string(), chain: true },
    }
}

// ---------------------------------------------------------------------------
// gis-geo (ge)
// ---------------------------------------------------------------------------

fn ge_manifest() -> OrganelleManifest {
    let pair = || {
        obj(json!({"lat1": {"type": "number"}, "lon1": {"type": "number"},
                   "lat2": {"type": "number"}, "lon2": {"type": "number"}}),
            &["lat1", "lon1", "lat2", "lon2"])
    };
    manifest_base(
        "gis-geo", "ge",
        "pure geodesy toolkit: haversine, bearings, destinations, tile math, projections",
        vec![
            contract("haversine", "haversine great-circle distance in meters", pair()),
            contract("bearing", "initial bearing in degrees", pair()),
            contract("destination", "destination point from bearing + distance",
                obj(json!({"lat": {"type": "number"}, "lon": {"type": "number"},
                           "bearing_deg": {"type": "number"}, "dist_m": {"type": "number"}}),
                    &["lat", "lon", "bearing_deg", "dist_m"])),
            contract("tile_xy", "slippy-map tile x,y at zoom z",
                obj(json!({"lat": {"type": "number"}, "lon": {"type": "number"}, "z": {"type": "integer"}}),
                    &["lat", "lon", "z"])),
            contract("tile_bbox", "tile bounding box as sw/ne points",
                obj(json!({"x": {"type": "integer"}, "y": {"type": "integer"}, "z": {"type": "integer"}}),
                    &["x", "y", "z"])),
            contract("to_meters", "WGS-84 to Web-Mercator meters",
                obj(json!({"lat": {"type": "number"}, "lon": {"type": "number"}}), &["lat", "lon"])),
            contract("to_latlon", "Web-Mercator meters to WGS-84",
                obj(json!({"x": {"type": "number"}, "y": {"type": "number"}}), &["x", "y"])),
        ],
        Valence::Proven,
    )
}

pub struct GisGeo {
    manifest: OrganelleManifest,
}

pub fn geo_organelle() -> Arc<dyn Organelle> {
    Arc::new(GisGeo { manifest: ge_manifest() })
}

fn pair_in(v: &Value) -> Result<(LatLon, LatLon), OrganelleError> {
    let a = LatLon::new(num_in(v, "lat1")?, num_in(v, "lon1")?).ok_or_else(|| failed("point A out of range".to_string()))?;
    let b = LatLon::new(num_in(v, "lat2")?, num_in(v, "lon2")?).ok_or_else(|| failed("point B out of range".to_string()))?;
    Ok((a, b))
}

impl Organelle for GisGeo {
    fn manifest(&self) -> OrganelleManifest {
        self.manifest.clone()
    }
    fn handle(&self, _ctx: &CallContext, _host: &dyn HostRef, op: &str, input: Value) -> Result<Value, OrganelleError> {
        match op {
            "haversine" => {
                let (a, b) = pair_in(&input)?;
                Ok(json!({ "meters": a.distance_m(&b) }))
            }
            "bearing" => {
                let (a, b) = pair_in(&input)?;
                Ok(json!({ "bearing_deg": a.bearing_deg(&b) }))
            }
            "destination" => {
                let p = latlon_in(&input)?;
                let d = p.destination(num_in(&input, "bearing_deg")?, num_in(&input, "dist_m")?);
                Ok(json!({ "point": latlon_json(&d) }))
            }
            "tile_xy" => {
                let p = latlon_in(&input)?;
                let z = int_in(&input, "z")?;
                if !(0..=22).contains(&z) {
                    return Err(failed("z must be 0..=22".to_string()));
                }
                let (x, y) = WebMercator::tile_xy(&p, z as u8);
                Ok(json!({ "x": x, "y": y, "z": z }))
            }
            "tile_bbox" => {
                let (sw, ne) = WebMercator::tile_bbox(int_in(&input, "x")? as u32, int_in(&input, "y")? as u32, int_in(&input, "z")? as u8);
                Ok(json!({ "sw": latlon_json(&sw), "ne": latlon_json(&ne) }))
            }
            "to_meters" => {
                let (x, y) = WebMercator::to_meters(&latlon_in(&input)?);
                Ok(json!({ "x": x, "y": y }))
            }
            "to_latlon" => {
                let p = WebMercator::to_latlon(num_in(&input, "x")?, num_in(&input, "y")?);
                Ok(json!({ "point": latlon_json(&p) }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op {op}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// gis-poi (gi)
// ---------------------------------------------------------------------------

fn gi_manifest() -> OrganelleManifest {
    manifest_base(
        "gis-poi", "gi",
        "points-of-interest database: radius/kind queries, JSON persistence, seeded canonical datasets",
        vec![
            contract_gated("add", "add a POI -> id",
                obj(json!({"name": {"type": "string"}, "kind": {"type": "string"},
                           "lat": {"type": "number"}, "lon": {"type": "number"}, "notes": {"type": "string"}}),
                    &["name", "kind", "lat", "lon"]), "gi.write"),
            contract_gated("remove", "remove a POI by id",
                obj(json!({"id": {"type": "string"}}), &["id"]), "gi.write"),
            contract("get", "get one POI by id",
                obj(json!({"id": {"type": "string"}}), &["id"])),
            contract("near", "POIs within radius_m of a point, with distances",
                obj(json!({"lat": {"type": "number"}, "lon": {"type": "number"}, "radius_m": {"type": "number"}}),
                    &["lat", "lon", "radius_m"])),
            contract("by_kind", "POIs of one kind",
                obj(json!({"kind": {"type": "string"}}), &["kind"])),
            contract("user_added", "user-added POIs", obj(json!({}), &[])),
            contract_gated("seed_saltspring", "seed the saltspring canonical dataset",
                obj(json!({}), &[]), "gi.write"),
            contract_gated("seed_coombs", "seed the coombs canonical dataset",
                obj(json!({}), &[]), "gi.write"),
            contract_gated("save", "persist the DB to a JSON file",
                obj(json!({"path": {"type": "string"}}), &["path"]), "gi.write"),
            contract("load", "load the DB from a JSON file (replaces in-memory state)",
                obj(json!({"path": {"type": "string"}}), &["path"])),
        ],
        Valence::Candidate,
    )
}

fn poi_json(p: &poi::Poi) -> Value {
    json!({
        "id": p.id, "name": p.name, "kind": p.kind,
        "lat": p.pos.lat, "lon": p.pos.lon,
        "notes": p.notes, "user_added": p.user_added, "created_unix": p.created_unix,
    })
}

pub struct GisPoi {
    manifest: OrganelleManifest,
    db: Mutex<poi::PoiDb>,
}

pub fn poi_organelle() -> Arc<dyn Organelle> {
    Arc::new(GisPoi { manifest: gi_manifest(), db: Mutex::new(poi::PoiDb::new()) })
}

impl Organelle for GisPoi {
    fn manifest(&self) -> OrganelleManifest {
        self.manifest.clone()
    }
    fn handle(&self, _ctx: &CallContext, _host: &dyn HostRef, op: &str, input: Value) -> Result<Value, OrganelleError> {
        let mut db = self.db.lock().map_err(|_| failed("poi db lock poisoned".to_string()))?;
        match op {
            "add" => {
                let id = db.add(&str_in(&input, "name")?, &str_in(&input, "kind")?,
                    num_in(&input, "lat")?, num_in(&input, "lon")?,
                    input.get("notes").and_then(|v| v.as_str()).unwrap_or(""), true)
                    .map_err(failed)?;
                Ok(json!({ "id": id }))
            }
            "remove" => Ok(json!({ "removed": db.remove(&str_in(&input, "id")?) })),
            "get" => match db.get(&str_in(&input, "id")?) {
                Some(p) => Ok(json!({ "found": true, "poi": poi_json(p) })),
                None => Ok(json!({ "found": false })),
            },
            "near" => {
                let center = latlon_in(&input)?;
                let hits: Vec<Value> = db.near(&center, num_in(&input, "radius_m")?)
                    .into_iter().map(|(p, d)| json!({ "poi": poi_json(p), "distance_m": d })).collect();
                Ok(json!({ "hits": hits }))
            }
            "by_kind" => {
                let hits: Vec<Value> = db.by_kind(&str_in(&input, "kind")?).into_iter().map(poi_json).collect();
                Ok(json!({ "hits": hits }))
            }
            "user_added" => {
                let hits: Vec<Value> = db.user_added().into_iter().map(poi_json).collect();
                Ok(json!({ "hits": hits }))
            }
            "seed_saltspring" => { db.seed_saltspring(); Ok(json!({ "seeded": "saltspring", "count": db.len() })) }
            "seed_coombs" => { db.seed_coombs(); Ok(json!({ "seeded": "coombs", "count": db.len() })) }
            "save" => { db.save(&str_in(&input, "path")?).map_err(|e| failed(e.to_string()))?; Ok(json!({ "saved": true })) }
            "load" => {
                let loaded = poi::PoiDb::load(&str_in(&input, "path")?).map_err(|e| failed(e.to_string()))?;
                let n = loaded.len();
                *db = loaded;
                Ok(json!({ "loaded": true, "count": n }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op {op}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// gis-route (gr)
// ---------------------------------------------------------------------------

fn gr_manifest() -> OrganelleManifest {
    manifest_base(
        "gis-route", "gj",
        "pedicab route planner: waypoint graph, grade-penalized Dijkstra",
        vec![
            contract_gated("add_node", "add a waypoint node",
                obj(json!({"id": {"type": "string"}, "name": {"type": "string"},
                           "lat": {"type": "number"}, "lon": {"type": "number"}}),
                    &["id", "name", "lat", "lon"]), "gj.write"),
            contract_gated("add_leg", "add a leg between waypoints with grade 0..1",
                obj(json!({"a": {"type": "string"}, "b": {"type": "string"}, "grade": {"type": "number"}}),
                    &["a", "b", "grade"]), "gj.write"),
            contract("shortest", "Dijkstra shortest path under a profile",
                obj(json!({"from": {"type": "string"}, "to": {"type": "string"},
                           "profile": {"type": "string", "enum": ["pedicab", "direct"]}}),
                    &["from", "to", "profile"])),
            contract("path_distance", "recompute a path's length from its legs",
                obj(json!({"path": {"type": "array", "items": {"type": "string"}}}), &["path"])),
        ],
        Valence::Candidate,
    )
}

pub struct GisRoute {
    manifest: OrganelleManifest,
    graph: Mutex<route::Graph>,
}

pub fn route_organelle() -> Arc<dyn Organelle> {
    Arc::new(GisRoute { manifest: gr_manifest(), graph: Mutex::new(route::Graph::default()) })
}

impl Organelle for GisRoute {
    fn manifest(&self) -> OrganelleManifest {
        self.manifest.clone()
    }
    fn handle(&self, _ctx: &CallContext, _host: &dyn HostRef, op: &str, input: Value) -> Result<Value, OrganelleError> {
        let mut g = self.graph.lock().map_err(|_| failed("route graph lock poisoned".to_string()))?;
        match op {
            "add_node" => {
                g.add_node(&str_in(&input, "id")?, &str_in(&input, "name")?, latlon_in(&input)?);
                Ok(json!({ "added": true }))
            }
            "add_leg" => {
                g.add_leg(&str_in(&input, "a")?, &str_in(&input, "b")?, num_in(&input, "grade")?);
                Ok(json!({ "added": true }))
            }
            "shortest" => {
                let profile = match input.get("profile").and_then(|v| v.as_str()).unwrap_or("") {
                    "pedicab" => route::Profile::Pedicab,
                    "direct" => route::Profile::Direct,
                    s => return Err(failed(format!("unknown profile {s:?}"))),
                };
                match g.shortest(&str_in(&input, "from")?, &str_in(&input, "to")?, profile) {
                    Some(path) => {
                        let dist = g.path_distance_m(&path);
                        Ok(json!({ "found": true, "path": path, "distance_m": dist }))
                    }
                    None => Ok(json!({ "found": false })),
                }
            }
            "path_distance" => {
                let path: Vec<String> = input.get("path").and_then(|v| serde_json::from_value(v.clone()).ok())
                    .ok_or_else(|| failed("missing path array".to_string()))?;
                Ok(json!({ "distance_m": g.path_distance_m(&path) }))
            }
            _ => Err(OrganelleError::not_found(format!("unknown op {op}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use organelle_host::Host;
    use organelle_manifest::validate_manifest_json;

    fn manifest_json(name: &str) -> Value {
        let txt = std::fs::read_to_string(format!("../../organelles/{name}.manifest.json")).expect("manifest reads");
        serde_json::from_str(&txt).expect("manifest JSON parses")
    }

    #[test]
    fn static_manifests_validate() {
        for (name, codon) in [("gis-geo", "ge"), ("gis-poi", "gi"), ("gis-route", "gj")] {
            let m = validate_manifest_json(&manifest_json(name)).expect("static manifest must validate");
            assert_eq!(m.codon, codon);
        }
    }

    #[test]
    fn rust_manifests_match_static_json() {
        for (rust, name) in [(ge_manifest(), "gis-geo"), (gi_manifest(), "gis-poi"), (gr_manifest(), "gis-route")] {
            assert_eq!(serde_json::to_value(rust).unwrap(), manifest_json(name), "manifests must agree for {name}");
        }
    }

    #[test]
    fn geo_serves_pure_geodesy_with_receipts() {
        let host = Host::new();
        host.register(geo_organelle()).expect("register");
        let ctx = CallContext::new("ge-test", vec![]);
        // Vancouver -> Victoria approx 100km; real haversine
        let d = host.call(&ctx, "ge", "haversine", json!({"lat1": 49.28, "lon1": -123.12, "lat2": 48.42, "lon2": -123.36})).expect("haversine").output;
        let m = d["meters"].as_f64().unwrap();
        assert!(m > 90_000.0 && m < 115_000.0, "got {m}");
        let t = host.call(&ctx, "ge", "tile_xy", json!({"lat": 49.28, "lon": -123.12, "z": 12})).expect("tile").output;
        assert!(t["x"].as_u64().unwrap() > 0);
        assert!(host.call(&ctx, "ge", "haversine", json!({"lat1": 91.0, "lon1": 0.0, "lat2": 0.0, "lon2": 0.0})).is_err());
        assert!(host.verify_chain().is_ok());
    }

    #[test]
    fn poi_roundtrips_with_receipts() {
        let host = Host::new();
        host.register(poi_organelle()).expect("register");
        let ctx = CallContext::new("gi-test", vec!["gi.write".to_string()]);
        let a = host.call(&ctx, "gi", "add", json!({"name": "Pier", "kind": "dock", "lat": 49.28, "lon": -123.12, "notes": "ferry"})).expect("add").output;
        let id = a["id"].as_str().unwrap().to_string();
        let g = host.call(&ctx, "gi", "get", json!({"id": id})).expect("get").output;
        assert_eq!(g["poi"]["name"], "Pier");
        let n = host.call(&ctx, "gi", "near", json!({"lat": 49.281, "lon": -123.121, "radius_m": 500.0})).expect("near").output;
        assert_eq!(n["hits"].as_array().unwrap().len(), 1);
        let k = host.call(&ctx, "gi", "by_kind", json!({"kind": "dock"})).expect("by_kind").output;
        assert_eq!(k["hits"].as_array().unwrap().len(), 1);
        // save/load roundtrip through the real JSON persistence
        let dir = std::env::temp_dir().join("gi-test-pois.json");
        host.call(&ctx, "gi", "save", json!({"path": dir.to_str().unwrap()})).expect("save");
        host.call(&ctx, "gi", "remove", json!({"id": id})).expect("remove");
        host.call(&ctx, "gi", "load", json!({"path": dir.to_str().unwrap()})).expect("load");
        let g2 = host.call(&ctx, "gi", "get", json!({"id": id})).expect("get2").output;
        assert_eq!(g2["found"], true);
        assert!(host.verify_chain().is_ok());
    }

    #[test]
    fn route_plans_with_receipts() {
        let host = Host::new();
        host.register(route_organelle()).expect("register");
        let ctx = CallContext::new("gr-test", vec!["gj.write".to_string()]);
        for (id, name, lat, lon) in [("a", "A", 49.28, -123.12), ("b", "B", 49.29, -123.13), ("c", "C", 49.30, -123.14)] {
            host.call(&ctx, "gj", "add_node", json!({"id": id, "name": name, "lat": lat, "lon": lon})).expect("add_node");
        }
        host.call(&ctx, "gj", "add_leg", json!({"a": "a", "b": "b", "grade": 0.1})).expect("leg");
        host.call(&ctx, "gj", "add_leg", json!({"a": "b", "b": "c", "grade": 0.9})).expect("leg");
        host.call(&ctx, "gj", "add_leg", json!({"a": "a", "b": "c", "grade": 0.2})).expect("leg");
        // pedicab profile should prefer a->c direct (grade 0.2) over a->b->c (0.9 leg)
        let s = host.call(&ctx, "gj", "shortest", json!({"from": "a", "to": "c", "profile": "pedicab"})).expect("shortest").output;
        assert_eq!(s["found"], true);
        assert!(s["distance_m"].as_f64().unwrap() > 0.0);
        let miss = host.call(&ctx, "gj", "shortest", json!({"from": "a", "to": "zzz", "profile": "direct"})).expect("miss").output;
        assert_eq!(miss["found"], false);
        assert!(host.verify_chain().is_ok());
    }
}
