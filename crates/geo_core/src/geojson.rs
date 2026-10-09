//! Minimal GeoJSON builder (dependency-free, manual escaping).

use crate::coord::LatLon;

/// JSON-escape a string.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

#[derive(Debug, Clone)]
pub enum Geometry {
    Point(LatLon),
    LineString(Vec<LatLon>),
    Polygon(Vec<Vec<LatLon>>), // rings; first = exterior
}

impl Geometry {
    fn to_json(&self) -> String {
        fn pt(p: &LatLon) -> String {
            format!("[{:.6},{:.6}]", p.lon, p.lat)
        }
        match self {
            Geometry::Point(p) => format!("{{\"type\":\"Point\",\"coordinates\":{}}}", pt(p)),
            Geometry::LineString(v) => {
                let c: Vec<String> = v.iter().map(pt).collect();
                format!("{{\"type\":\"LineString\",\"coordinates\":[{}]}}", c.join(","))
            }
            Geometry::Polygon(rings) => {
                let r: Vec<String> = rings
                    .iter()
                    .map(|ring| {
                        let c: Vec<String> = ring.iter().map(pt).collect();
                        format!("[{}]", c.join(","))
                    })
                    .collect();
                format!("{{\"type\":\"Polygon\",\"coordinates\":[{}]}}", r.join(","))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Feature {
    pub geom: Geometry,
    pub props: Vec<(String, String)>,
}

impl Feature {
    pub fn new(geom: Geometry) -> Self {
        Self { geom, props: Vec::new() }
    }

    pub fn prop(mut self, k: &str, v: &str) -> Self {
        self.props.push((k.to_string(), v.to_string()));
        self
    }

    fn to_json(&self) -> String {
        let p: Vec<String> = self
            .props
            .iter()
            .map(|(k, v)| format!("\"{}\":\"{}\"", esc(k), esc(v)))
            .collect();
        format!(
            "{{\"type\":\"Feature\",\"geometry\":{},\"properties\":{{{}}}}}",
            self.geom.to_json(),
            p.join(",")
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct FeatureCollection {
    pub features: Vec<Feature>,
}

impl FeatureCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, f: Feature) {
        self.features.push(f);
    }

    pub fn to_json(&self) -> String {
        let f: Vec<String> = self.features.iter().map(|x| x.to_json()).collect();
        format!("{{\"type\":\"FeatureCollection\",\"features\":[{}]}}", f.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_feature_json_shape() {
        let mut fc = FeatureCollection::new();
        fc.push(
            Feature::new(Geometry::Point(LatLon::new_unchecked(48.85, -123.49)))
                .prop("name", "Ganges \"harbour\""),
        );
        let j = fc.to_json();
        assert!(j.contains("\"type\":\"FeatureCollection\""));
        assert!(j.contains("[-123.490000,48.850000]"));
        assert!(j.contains("Ganges \\\"harbour\\\""));
    }

    #[test]
    fn polygon_ring_closes() {
        let a = LatLon::new_unchecked(48.9, -123.6);
        let b = LatLon::new_unchecked(48.9, -123.4);
        let c = LatLon::new_unchecked(48.8, -123.4);
        let ring = vec![a, b, c, a];
        let mut fc = FeatureCollection::new();
        fc.push(Feature::new(Geometry::Polygon(vec![ring])));
        let j = fc.to_json();
        assert!(j.contains("\"type\":\"Polygon\""));
    }
}
