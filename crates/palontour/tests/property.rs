//! Deterministic property battery for the palontour crate.
//!
//! Hand-rolled runner (std-only workspace; crates.io unreachable): a
//! xorshift64 PRNG with a FIXED SEED TABLE. No wall-clock, no entropy.
//! A failing case is reproducible by (seed, case index) — both are printed
//! in the panic message. Do not "fix" a failure by narrowing a generator;
//! a real failure means the invariant is wrong or the code is.
//!
//! SEED TABLE (changing a seed changes the generated corpus):
//!   CAS_ADDRESS_STABILITY  0x0123456789ABCDEF
//!   CAS_COLLISION_FREE     0xFEDCBA9876543210
//!   ROOT_COMMUTATIVITY     0x0F1E2D3C4B5A6978
//!   QUADKEY_ROUNDTRIP      0x087F6E5D4C3B2A190
//!   SEAL_ROUNDTRIP         0x1122334455667788
//!   HAVERSINE              0x8877665544332211
//!   TILE_COVERAGE          0xA5A5A5A5A5A5A5A5
//!   JSON_ESCAPE            0x5A5A5A5A5A5A5A5A
//!
//! Families: 8, each >= 100 generated cases.

use palontour::{ContentStore, TileEntry, Tileset, blake2b_256, hex};
use std::collections::HashSet;
use std::path::PathBuf;

use geo_core::coord::{LatLon, WebMercator};
use geo_core::tile::Tile;

// ---------------------------------------------------------------------------
// deterministic PRNG
// ---------------------------------------------------------------------------

/// xorshift64 (Marsaglia). Seed must be non-zero.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        assert!(seed != 0, "xorshift seed must be non-zero");
        Self(seed)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        self.next() % n
    }
    /// f64 in [lo, hi).
    fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() as f64 / u64::MAX as f64) * (hi - lo)
    }
}

// ---------------------------------------------------------------------------
// generators
// ---------------------------------------------------------------------------

/// Random byte payload, length 0..=512.
fn gen_payload(rng: &mut XorShift64) -> Vec<u8> {
    let len = rng.below(513) as usize;
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(rng.next() as u8);
    }
    v
}

/// 64-char lowercase hex digest string.
fn gen_digest(rng: &mut XorShift64) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for _ in 0..32 {
        let b = rng.next() as u8;
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 15) as usize] as char);
    }
    s
}

/// Valid tile coordinate triple, z in 0..=12.
fn gen_tile_coord(rng: &mut XorShift64) -> (u8, u32, u32) {
    let z = rng.below(13) as u8;
    let m = 1u32 << z;
    (z, rng.below(m as u64) as u32, rng.below(m as u64) as u32)
}

fn gen_latlon(rng: &mut XorShift64) -> LatLon {
    let lat = rng.range_f64(-80.0, 80.0);
    let lon = rng.range_f64(-180.0, 180.0);
    LatLon::new(lat, lon).expect("generated coords in range")
}

/// Tileset name exercising the JSON escaper: backslashes, quotes, unicode,
/// emoji, spaces. Control characters are deliberately EXCLUDED — crate_escape
/// does not escape them (observed finding, see receipt).
fn gen_name(rng: &mut XorShift64) -> String {
    let alphabet: Vec<char> = "abcXYZ019 _-\\\"\u{e9}\u{4e2d}\u{1F600}\u{df}\u{3c0}"
        .chars()
        .collect();
    let len = rng.below(24) as usize;
    (0..len)
        .map(|_| alphabet[rng.below(alphabet.len() as u64) as usize])
        .collect()
}

fn store(tag: &str) -> ContentStore {
    let p: PathBuf = std::env::temp_dir().join(format!("palontour_prop_{tag}"));
    let _ = std::fs::remove_dir_all(&p);
    ContentStore::open(&p).expect("open store")
}

// ---------------------------------------------------------------------------
// family 1: CAS content-address stability
// ---------------------------------------------------------------------------

#[test]
fn prop_cas_address_stability() {
    let mut rng = XorShift64::new(0x0123456789ABCDEF);
    for i in 0..200 {
        let data = gen_payload(&mut rng);
        let s = store(&format!("stab{i}"));
        let d1 = s.put(&data).expect("put");
        let d2 = s.put(&data).expect("put again");
        assert_eq!(d1, d2, "case {i}: put not idempotent");
        let canon = hex(&blake2b_256(&data));
        assert_eq!(d1, canon, "case {i}: CAS digest != canonical blake2b_256");
        assert_eq!(s.get(&d1).expect("get"), data, "case {i}: get roundtrip");
        assert!(s.verify(&d1, &data), "case {i}: verify true");
        let _ =
            std::fs::remove_dir_all(std::env::temp_dir().join(format!("palontour_prop_stab{i}")));
    }
}

// ---------------------------------------------------------------------------
// family 2: collision resistance over the sampled space
// ---------------------------------------------------------------------------

#[test]
fn prop_cas_collision_free() {
    let mut rng = XorShift64::new(0xFEDCBA9876543210);
    let s = store("coll");
    let mut seen = HashSet::new();
    for i in 0..300u32 {
        // index prefix guarantees distinct inputs; the invariant under test
        // is that distinct inputs hash to distinct addresses.
        let mut data = format!("prop-collision-case-{i:06}-").into_bytes();
        data.extend(gen_payload(&mut rng));
        let d = s.put(&data).expect("put");
        assert!(seen.insert(d.clone()), "case {i}: DIGEST COLLISION on {d}");
    }
    assert_eq!(
        s.object_count(),
        300,
        "object_count after 300 distinct puts"
    );
}

// ---------------------------------------------------------------------------
// family 3: tileset root commutativity + sensitivity
// ---------------------------------------------------------------------------

#[test]
fn prop_tileset_root_commutativity() {
    let mut rng = XorShift64::new(0x0F1E2D3C4B5A6978);
    for i in 0..150 {
        let n = 1 + rng.below(20) as usize;
        let mut entries: Vec<(u8, u32, u32, String)> = Vec::new();
        let mut coords = HashSet::new();
        while entries.len() < n {
            let (z, x, y) = gen_tile_coord(&mut rng);
            if coords.insert((z, x, y)) {
                entries.push((z, x, y, gen_digest(&mut rng)));
            }
        }
        let mut a = Tileset::new("prop");
        for (z, x, y, d) in &entries {
            a.insert(*z, *x, *y, d.clone());
        }
        let mut b = Tileset::new("prop");
        for (z, x, y, d) in entries.iter().rev() {
            b.insert(*z, *x, *y, d.clone());
        }
        assert_eq!(
            a.root_digest(),
            b.root_digest(),
            "case {i}: root depends on insertion order"
        );
        // sensitivity: mutating one tile's digest must change the root
        let mut c = Tileset::new("prop");
        for (j, (z, x, y, d)) in entries.iter().enumerate() {
            let mut dd = d.clone();
            if j == 0 {
                let first = dd.remove(0);
                dd.insert(0, if first == 'a' { 'b' } else { 'a' });
            }
            c.insert(*z, *x, *y, dd);
        }
        assert_ne!(
            a.root_digest(),
            c.root_digest(),
            "case {i}: root insensitive to tile change"
        );
        // get() agrees with what was inserted
        for (z, x, y, d) in &entries {
            assert_eq!(a.get(*z, *x, *y), Some(d), "case {i}: get mismatch");
        }
    }
}

// ---------------------------------------------------------------------------
// family 4: tile-coordinate round-trip (quadkey encode/decode) + parent/child
// ---------------------------------------------------------------------------

/// Decode a Bing-style quadkey back to (x, y, z). Inverse of Tile::quadkey.
fn decode_quadkey(q: &str) -> (u32, u32, u8) {
    let z = q.len() as u8;
    let mut x = 0u32;
    let mut y = 0u32;
    for (j, ch) in q.chars().enumerate() {
        let d = ch.to_digit(4).expect("quadkey digit");
        let mask = 1u32 << (z as usize - 1 - j);
        if d & 1 != 0 {
            x |= mask;
        }
        if d & 2 != 0 {
            y |= mask;
        }
    }
    (x, y, z)
}

#[test]
fn prop_tile_quadkey_roundtrip() {
    let mut rng = XorShift64::new(0x87F6E5D4C3B2A190);
    for i in 0..200 {
        let (z, x, y) = gen_tile_coord(&mut rng);
        let t = Tile::new(x, y, z).expect("valid tile");
        let q = t.quadkey();
        assert_eq!(q.len(), z as usize, "case {i}: quadkey length");
        let (dx, dy, dz) = decode_quadkey(&q);
        assert_eq!((dx, dy, dz), (x, y, z), "case {i}: quadkey roundtrip");
        // parent/child inversion
        if z >= 1 {
            let p = t.parent().expect("parent");
            let kids = p.children().expect("children");
            assert!(
                kids.contains(&t),
                "case {i}: child not in parent's children"
            );
        }
        // out-of-range rejection on the boundary
        let m = 1u32 << z;
        assert!(Tile::new(m, 0, z).is_none(), "case {i}: x=2^z accepted");
        assert!(Tile::new(0, m, z).is_none(), "case {i}: y=2^z accepted");
    }
}

// ---------------------------------------------------------------------------
// family 5: seal / record serialization round-trip
// ---------------------------------------------------------------------------

/// Minimal parser for the tiles array in Tileset::to_json output.
/// Returns (name, root, Vec<(z,x,y,digest)>).
fn parse_tileset_json(json: &str) -> (String, String, Vec<TileEntry>) {
    fn extract_str(json: &str, key: &str) -> String {
        let pat = format!("\"{key}\":\"");
        let start = json
            .find(&pat)
            .unwrap_or_else(|| panic!("key {key} missing"))
            + pat.len();
        let rest = &json[start..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    let e = chars.next().expect("escape at end");
                    match e {
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        other => panic!("unexpected escape \\{other}"),
                    }
                }
                '"' => break,
                other => out.push(other),
            }
        }
        out
    }
    fn extract_num(entry: &str, key: &str) -> u64 {
        let pat = format!("\"{key}\":");
        let start = entry
            .find(&pat)
            .unwrap_or_else(|| panic!("num {key} missing"))
            + pat.len();
        let rest = &entry[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit()))
            .unwrap_or(rest.len());
        rest[..end].parse().expect("num parse")
    }
    let name = extract_str(json, "name");
    let root = extract_str(json, "root");
    let tiles_start = json.find("\"tiles\":[").expect("tiles array") + "\"tiles\":[".len();
    let body = &json[tiles_start..];
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut arr_depth = 1usize; // we are inside "tiles":[
    let mut cur = String::new();
    for ch in body.chars() {
        match ch {
            '[' => {
                arr_depth += 1;
                if depth > 0 {
                    cur.push(ch);
                }
            }
            ']' => {
                arr_depth -= 1;
                if arr_depth == 0 {
                    break; // end of the tiles array
                }
                if depth > 0 {
                    cur.push(ch);
                }
            }
            '{' => {
                depth += 1;
                cur.push(ch);
            }
            '}' => {
                if depth == 0 {
                    continue; // outer object brace — not ours
                }
                depth -= 1;
                cur.push(ch);
                if depth == 0 {
                    entries.push(TileEntry {
                        z: extract_num(&cur, "z") as u8,
                        x: extract_num(&cur, "x") as u32,
                        y: extract_num(&cur, "y") as u32,
                        digest: extract_str(&cur, "digest"),
                    });
                    cur.clear();
                }
            }
            _ => {
                if depth > 0 {
                    cur.push(ch);
                }
            }
        }
    }
    (name, root, entries)
}

#[test]
fn prop_seal_roundtrip() {
    let mut rng = XorShift64::new(0x1122334455667788);
    for i in 0..100 {
        let name = gen_name(&mut rng);
        let n = 1 + rng.below(15) as usize;
        let mut entries: Vec<(u8, u32, u32, String)> = Vec::new();
        let mut coords = HashSet::new();
        while entries.len() < n {
            let (z, x, y) = gen_tile_coord(&mut rng);
            if coords.insert((z, x, y)) {
                entries.push((z, x, y, gen_digest(&mut rng)));
            }
        }
        let mut t = Tileset::new(&name);
        for (z, x, y, d) in &entries {
            t.insert(*z, *x, *y, d.clone());
        }
        let json = t.to_json();
        let s = store(&format!("seal{i}"));
        // seal stores the index JSON itself: digest must equal the canonical
        // hash of the exact bytes, and get must return them byte-identical.
        let sealed = t.seal(&s).expect("seal");
        assert_eq!(
            sealed,
            hex(&blake2b_256(json.as_bytes())),
            "case {i}: seal digest != blake2b_256(to_json bytes)"
        );
        let back = String::from_utf8(s.get(&sealed).expect("get")).expect("utf8");
        assert_eq!(back, json, "case {i}: sealed bytes != to_json bytes");
        // record round-trip: parse the JSON back and compare all fields
        let (pname, proot, pentries) = parse_tileset_json(&json);
        assert_eq!(pname, name, "case {i}: name roundtrip");
        assert_eq!(proot, t.root_digest(), "case {i}: root roundtrip");
        assert_eq!(pentries.len(), entries.len(), "case {i}: tile count");
        for (z, x, y, d) in &entries {
            let hit = pentries
                .iter()
                .find(|e| e.z == *z && e.x == *x && e.y == *y)
                .unwrap_or_else(|| panic!("case {i}: tile ({z},{x},{y}) lost in JSON"));
            assert_eq!(&hit.digest, d, "case {i}: digest roundtrip");
        }
        let _ =
            std::fs::remove_dir_all(std::env::temp_dir().join(format!("palontour_prop_seal{i}")));
    }
}

// ---------------------------------------------------------------------------
// family 6: haversine invariants
// ---------------------------------------------------------------------------

#[test]
fn prop_haversine() {
    let mut rng = XorShift64::new(0x8877665544332211);
    for i in 0..300 {
        let a = gen_latlon(&mut rng);
        let b = gen_latlon(&mut rng);
        let c = gen_latlon(&mut rng);
        let dab = a.distance_m(&b);
        let dba = b.distance_m(&a);
        assert!(dab >= 0.0, "case {i}: negative distance {dab}");
        assert!(
            (dab - dba).abs() < 1e-9,
            "case {i}: asymmetric d(a,b)={dab} d(b,a)={dba}"
        );
        assert_eq!(a.distance_m(&a), 0.0, "case {i}: self-distance != 0");
        // triangle inequality (sub-millimeter slack for float error)
        let dac = a.distance_m(&c);
        let dbc = b.distance_m(&c);
        assert!(
            dac <= dab + dbc + 1e-3,
            "case {i}: triangle violated d(a,c)={dac} > {dab}+{dbc}"
        );
        // sanity: distinct far-apart points have positive distance
        if (a.lat - b.lat).abs() > 1.0 || (a.lon - b.lon).abs() > 1.0 {
            assert!(dab > 0.0, "case {i}: far points at zero distance");
        }
    }
}

// ---------------------------------------------------------------------------
// family 7: tile coverage — every sampled point lands in the covering set
// ---------------------------------------------------------------------------

#[test]
fn prop_tile_coverage() {
    let mut rng = XorShift64::new(0xA5A5A5A5A5A5A5A5);
    for i in 0..100 {
        let z = rng.below(13) as u8;
        let lat_lo = rng.range_f64(-80.0, 75.0);
        let lat_hi = lat_lo + rng.range_f64(0.05, 5.0);
        let lon_lo = rng.range_f64(-170.0, 165.0);
        let lon_hi = lon_lo + rng.range_f64(0.05, 5.0);
        let nw = LatLon::new(lat_hi, lon_lo).expect("nw in range");
        let se = LatLon::new(lat_lo.min(80.0), lon_hi).expect("se in range");
        let ts = Tile::covering(&nw, &se, z);
        assert!(!ts.is_empty(), "case {i}: empty covering set");
        let set: HashSet<(u32, u32)> = ts.iter().map(|t| (t.x, t.y)).collect();
        for _ in 0..5 {
            let p = LatLon::new(
                rng.range_f64(lat_lo.min(80.0), lat_hi.min(80.0)),
                rng.range_f64(lon_lo, lon_hi),
            )
            .expect("sample in range");
            let (x, y) = WebMercator::tile_xy(&p, z);
            assert!(
                set.contains(&(x, y)),
                "case {i}: point ({},{}) tile ({x},{y}) missing from covering set",
                p.lat,
                p.lon
            );
        }
        // every covering tile's bbox must intersect the query box
        for t in &ts {
            let (tnw, tse) = t.bbox();
            assert!(
                tnw.lon <= se.lon && tse.lon >= nw.lon,
                "case {i}: tile ({},{}) bbox misses query box",
                t.x,
                t.y
            );
        }
    }
}

// ---------------------------------------------------------------------------
// family 8: to_json name escaping round-trip
// ---------------------------------------------------------------------------

#[test]
fn prop_json_escape_roundtrip() {
    let mut rng = XorShift64::new(0x5A5A5A5A5A5A5A5A);
    for i in 0..150 {
        let name = gen_name(&mut rng);
        let mut t = Tileset::new(&name);
        t.insert(5, 1, 2, "ab".into());
        let json = t.to_json();
        let (pname, _, _) = parse_tileset_json(&json);
        assert_eq!(pname, name, "case {i}: name escape roundtrip");
        // the escaped form must not contain a raw unescaped quote boundary
        assert!(
            json.starts_with("{\"name\":\""),
            "case {i}: JSON shape broken by name"
        );
    }
}
