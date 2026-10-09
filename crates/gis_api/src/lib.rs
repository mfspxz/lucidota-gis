//! gis_api — gis-017: keyed REST API over std TcpListener (no external crates).
//!
//! Auth: X-API-Key header, constant-time compare. /v1/health is open;
//! everything else 401s without the key. Privacy by default.

use geo_core::coord::LatLon;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiState {
    pub api_key: String,
    pub db: Arc<poi::PoiDb>,
    pub graph: Arc<route::Graph>,
    pub layers: Arc<Vec<(String, String)>>,
    pub tiles: Arc<HashMap<(u8, u32, u32), Vec<u8>>>,
}

impl ApiState {
    pub fn demo(key: &str) -> Self {
        let mut db = poi::PoiDb::new();
        db.seed_saltspring();
        db.seed_coombs();
        let graph = route::Graph::from_pois(&db, 12_000.0);
        let mut layers = saltspring::layers_geojson();
        layers.extend(coombs::layers_geojson());
        Self {
            api_key: key.into(),
            db: Arc::new(db),
            graph: Arc::new(graph),
            layers: Arc::new(layers),
            tiles: Arc::new(HashMap::new()),
        }
    }

    pub fn with_pack(mut self, pack_path: &str) -> Result<Self, String> {
        let info = offline_packs::PackReader::open(pack_path)?;
        let mut tiles = HashMap::new();
        for t in info.tiles {
            tiles.insert((t.z, t.x, t.y), t.png);
        }
        self.tiles = Arc::new(tiles);
        Ok(self)
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String, HashMap<String, String>)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 65536 {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    Ok((method, path, headers))
}

fn respond(stream: &mut TcpStream, status: u16, ctype: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json_esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn handle(state: &ApiState, stream: &mut TcpStream) {
    let (method, path, headers) = match read_request(stream) {
        Ok(r) => r,
        Err(_) => {
            respond(stream, 400, "text/plain", b"bad request");
            return;
        }
    };
    if method != "GET" {
        respond(stream, 405, "text/plain", b"method not allowed");
        return;
    }
    let (path_no_q, query) = match path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path.as_str(), ""),
    };
    let qmap: HashMap<&str, &str> = query.split('&').filter_map(|kv| kv.split_once('=')).collect();

    if path_no_q == "/v1/health" {
        respond(stream, 200, "application/json", b"{\"ok\":true}");
        return;
    }
    let authed = headers
        .get("x-api-key")
        .map(|k| constant_time_eq(k, &state.api_key))
        .unwrap_or(false);
    if !authed {
        respond(stream, 401, "application/json", b"{\"error\":\"unauthorized\"}");
        return;
    }

    if path_no_q == "/v1/layers" {
        let names: Vec<String> = state.layers.iter().map(|(n, _)| format!("\"{n}\"")).collect();
        respond(stream, 200, "application/json", format!("[{}]", names.join(",")).as_bytes());
    } else if let Some(name) = path_no_q.strip_prefix("/v1/layers/") {
        match state.layers.iter().find(|(n, _)| n == name) {
            Some((_, gj)) => respond(stream, 200, "application/geo+json", gj.as_bytes()),
            None => respond(stream, 404, "application/json", b"{\"error\":\"no such layer\"}"),
        }
    } else if path_no_q == "/v1/poi" {
        let mut j = String::from("[");
        for (i, p) in state.db.snapshot().iter().enumerate() {
            if i > 0 { j.push(','); }
            j.push_str(&format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"kind\":\"{}\",\"lat\":{:.6},\"lon\":{:.6}}}",
                p.id, json_esc(&p.name), json_esc(&p.kind), p.pos.lat, p.pos.lon
            ));
        }
        j.push(']');
        respond(stream, 200, "application/json", j.as_bytes());
    } else if path_no_q == "/v1/poi/near" {
        let lat: f64 = qmap.get("lat").and_then(|v| v.parse().ok()).unwrap_or(f64::NAN);
        let lon: f64 = qmap.get("lon").and_then(|v| v.parse().ok()).unwrap_or(f64::NAN);
        let r: f64 = qmap.get("r").and_then(|v| v.parse().ok()).unwrap_or(1000.0);
        let center = match LatLon::new(lat, lon) {
            Some(c) => c,
            None => {
                respond(stream, 400, "application/json", b"{\"error\":\"bad lat/lon\"}");
                return;
            }
        };
        let mut j = String::from("[");
        for (i, (p, d)) in state.db.near(&center, r).iter().enumerate() {
            if i > 0 { j.push(','); }
            j.push_str(&format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"dist_m\":{d:.1}}}",
                p.id, json_esc(&p.name)
            ));
        }
        j.push(']');
        respond(stream, 200, "application/json", j.as_bytes());
    } else if path_no_q == "/v1/route" {
        let from = qmap.get("from").copied().unwrap_or("");
        let to = qmap.get("to").copied().unwrap_or("");
        match state.graph.shortest(from, to, route::Profile::Pedicab) {
            Some(path) => {
                let d = state.graph.path_distance_m(&path);
                let nodes: Vec<String> = path.iter().map(|n| format!("\"{n}\"")).collect();
                let j = format!("{{\"path\":[{}],\"distance_m\":{d:.1},\"profile\":\"pedicab\"}}", nodes.join(","));
                respond(stream, 200, "application/json", j.as_bytes());
            }
            None => respond(stream, 404, "application/json", b"{\"error\":\"no route\"}"),
        }
    } else if let Some(rest) = path_no_q.strip_prefix("/v1/tile/") {
        let mut it = rest.split('/');
        let (z, x, y) = (
            it.next().and_then(|v| v.parse::<u8>().ok()),
            it.next().and_then(|v| v.parse::<u32>().ok()),
            it.next().and_then(|v| v.parse::<u32>().ok()),
        );
        match (z, x, y) {
            (Some(z), Some(x), Some(y)) => match state.tiles.get(&(z, x, y)) {
                Some(png) => respond(stream, 200, "image/png", png),
                None => respond(stream, 404, "application/json", b"{\"error\":\"no such tile\"}"),
            },
            _ => respond(stream, 400, "application/json", b"{\"error\":\"bad tile address\"}"),
        }
    } else {
        respond(stream, 404, "application/json", b"{\"error\":\"not found\"}");
    }
}

/// Serve exactly `n` connections then return (test harness). Production loops forever.
pub fn serve_n(listener: TcpListener, state: ApiState, n: usize) {
    for stream in listener.incoming().take(n) {
        if let Ok(mut s) = stream {
            handle(&state, &mut s);
        }
    }
}

pub fn bind_loopback() -> std::io::Result<(TcpListener, u16)> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    Ok((l, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn get(port: u16, path: &str, key: Option<&str>) -> (String, Vec<u8>) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut req = format!("GET {path} HTTP/1.1\r\nHost: x\r\n");
        if let Some(k) = key {
            req.push_str(&format!("X-API-Key: {k}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).unwrap();
        let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        (String::from_utf8_lossy(&buf[..head_end]).into_owned(), buf[head_end..].to_vec())
    }

    fn serve(state: ApiState, n: usize) -> u16 {
        let (l, port) = bind_loopback().unwrap();
        std::thread::spawn(move || serve_n(l, state, n));
        std::thread::sleep(std::time::Duration::from_millis(50));
        port
    }

    const KEY: &str = "test-key-123";

    #[test]
    fn health_open_layers_keyed() {
        let port = serve(ApiState::demo(KEY), 3);
        let (h, b) = get(port, "/v1/health", None);
        assert!(h.contains("200 OK") && b == b"{\"ok\":true}");
        let (h, _) = get(port, "/v1/layers", None);
        assert!(h.contains("401"), "layers must require key");
        let (h, b) = get(port, "/v1/layers", Some("wrong"));
        assert!(h.contains("401"), "wrong key must 401");
    }

    #[test]
    fn layers_and_poi_with_key() {
        let port = serve(ApiState::demo(KEY), 3);
        let (h, b) = get(port, "/v1/layers", Some(KEY));
        assert!(h.contains("200 OK"));
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("places") && s.contains("coombs_places"));
        let (h, b) = get(port, "/v1/layers/places", Some(KEY));
        assert!(h.contains("200 OK") && String::from_utf8_lossy(&b).contains("Ganges"));
        let (h, b) = get(port, "/v1/poi/near?lat=48.8541&lon=-123.4994&r=500", Some(KEY));
        assert!(h.contains("200 OK"));
        assert!(String::from_utf8_lossy(&b).contains("Ganges"));
    }

    #[test]
    fn route_endpoint() {
        let state = ApiState::demo(KEY);
        let ganges = state.db.near(&LatLon::new_unchecked(48.8541, -123.4994), 100.0)[0].0.id.clone();
        let fulford = state.db.near(&LatLon::new_unchecked(48.7677, -123.4500), 500.0)[0].0.id.clone();
        let port = serve(state, 1);
        let (h, b) = get(port, &format!("/v1/route?from={ganges}&to={fulford}"), Some(KEY));
        assert!(h.contains("200 OK"), "{h}");
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("\"profile\":\"pedicab\"") && s.contains("distance_m"));
    }

    #[test]
    fn tile_from_pack() {
        // build a tiny pack, serve it, fetch the tile bytes back
        let p = "/tmp/gis_api_pack.gispack";
        let r = geo_core::raster::Raster::filled(256, 256, 9, 9, 9);
        let mut png = Vec::new();
        r.write_png(&mut png).unwrap();
        let mut b = offline_packs::PackBuilder::new("t", "{}");
        b.add_tile(10, 301, 372, png.clone());
        b.build(p).unwrap();
        let state = ApiState::demo(KEY).with_pack(p).unwrap();
        let port = serve(state, 2);
        let (h, body) = get(port, "/v1/tile/10/301/372", Some(KEY));
        assert!(h.contains("200 OK") && h.contains("image/png"));
        assert_eq!(body, png);
        let (h, _) = get(port, "/v1/tile/10/999/999", Some(KEY));
        assert!(h.contains("404"));
    }
}
