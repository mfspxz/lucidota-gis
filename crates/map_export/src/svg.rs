//! SVG map renderer: Web-Mercator projection, scale bar, title, north arrow.

use geo_core::coord::{LatLon, WebMercator};

#[derive(Debug, Clone)]
pub enum SvgGeom {
    Point(LatLon, String),          // pos, label
    Line(Vec<LatLon>, String),      // pts, css color
    Poly(Vec<LatLon>, String),      // ring, css fill
}

pub struct SvgMap {
    pub width: u32,
    pub height: u32,
    pub nw: LatLon,
    pub se: LatLon,
    pub title: String,
    geoms: Vec<SvgGeom>,
}

impl SvgMap {
    pub fn new(width: u32, height: u32, nw: LatLon, se: LatLon, title: &str) -> Self {
        Self { width, height, nw, se, title: title.into(), geoms: Vec::new() }
    }

    fn project(&self, p: &LatLon) -> (f64, f64) {
        let (x0, y0) = WebMercator::to_meters(&self.nw);
        let (x1, y1) = WebMercator::to_meters(&self.se);
        let (x, y) = WebMercator::to_meters(p);
        let dx = x1 - x0;
        let dy = y1 - y0; // negative: screen y grows southward, meters grow northward
        let fx = if dx.abs() > 1e-9 { (x - x0) / dx } else { 0.0 };
        let fy = if dy.abs() > 1e-9 { (y - y0) / dy } else { 0.0 };
        (fx * self.width as f64, fy * self.height as f64)
    }

    pub fn add(&mut self, g: SvgGeom) {
        self.geoms.push(g);
    }

    pub fn render(&self) -> String {
        let mut s = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/>\n",
            self.width, self.height, self.width, self.height
        );
        for g in &self.geoms {
            match g {
                SvgGeom::Point(p, label) => {
                    let (x, y) = self.project(p);
                    s.push_str(&format!(
                        "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"5\" fill=\"#ff4fd8\" stroke=\"#fff\" stroke-width=\"1\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#e6edf3\" font-size=\"12\" font-family=\"monospace\">{}</text>\n",
                        x + 8.0, y + 4.0, label.replace('&', "&amp;").replace('<', "&lt;")
                    ));
                }
                SvgGeom::Line(pts, color) => {
                    let d: Vec<String> = pts.iter().map(|p| {
                        let (x, y) = self.project(p);
                        format!("{x:.1},{y:.1}")
                    }).collect();
                    s.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\"/>\n", d.join(" ")));
                }
                SvgGeom::Poly(ring, fill) => {
                    let d: Vec<String> = ring.iter().map(|p| {
                        let (x, y) = self.project(p);
                        format!("{x:.1},{y:.1}")
                    }).collect();
                    s.push_str(&format!("<polygon points=\"{}\" fill=\"{fill}\" fill-opacity=\"0.35\" stroke=\"{fill}\" stroke-width=\"1\"/>\n", d.join(" ")));
                }
            }
        }
        // scale bar: compute meters per pixel at mid latitude
        let mid = LatLon::new_unchecked((self.nw.lat + self.se.lat) / 2.0, (self.nw.lon + self.se.lon) / 2.0);
        let mpp = {
            let a = LatLon::new_unchecked(mid.lat, self.nw.lon);
            let b = LatLon::new_unchecked(mid.lat, self.se.lon);
            a.distance_m(&b) / self.width as f64
        };
        let bar_m = nice_scale(mpp * 120.0);
        let bar_px = bar_m / mpp;
        s.push_str(&format!(
            "<g font-family=\"monospace\" font-size=\"11\" fill=\"#e6edf3\">\n<rect x=\"20\" y=\"{}\" width=\"{bar_px:.0}\" height=\"6\" fill=\"#e6edf3\"/>\n<text x=\"20\" y=\"{}\">{}</text>\n</g>\n",
            self.height - 30, self.height - 38, fmt_dist(bar_m)
        ));
        // title + north arrow
        s.push_str(&format!(
            "<text x=\"20\" y=\"30\" fill=\"#ff4fd8\" font-size=\"20\" font-family=\"monospace\" font-weight=\"bold\">{}</text>\n<g transform=\"translate({},{})\" fill=\"#e6edf3\" font-family=\"monospace\" font-size=\"14\" text-anchor=\"middle\"><polygon points=\"0,-14 6,6 0,2 -6,6\" fill=\"#e6edf3\"/><text y=\"26\">N</text></g>\n",
            self.title.replace('&', "&amp;"), self.width - 40, 40
        ));
        s.push_str("</svg>\n");
        s
    }
}

fn nice_scale(raw: f64) -> f64 {
    let p = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * p >= raw { return m * p; }
    }
    10.0 * p
}

fn fmt_dist(m: f64) -> String {
    if m >= 1000.0 { format!("{:.0} km", m / 1000.0) } else { format!("{m:.0} m") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_projects_and_labels() {
        let nw = LatLon::new_unchecked(48.98, -123.72);
        let se = LatLon::new_unchecked(48.72, -123.28);
        let mut m = SvgMap::new(800, 600, nw, se, "Saltspring");
        m.add(SvgGeom::Point(LatLon::new_unchecked(48.8541, -123.4994), "Ganges".into()));
        m.add(SvgGeom::Line(vec![nw, se], "#ff4fd8".into()));
        let s = m.render();
        assert!(s.contains("Ganges"));
        assert!(s.contains("<circle"));
        assert!(s.contains("N</text>"));
        assert!(s.contains("km") || s.contains(" m<"));
    }

    #[test]
    fn projection_inside_canvas() {
        let nw = LatLon::new_unchecked(49.0, -124.0);
        let se = LatLon::new_unchecked(48.0, -123.0);
        let m = SvgMap::new(800, 600, nw, se, "t");
        let (x, y) = m.project(&LatLon::new_unchecked(48.5, -123.5));
        assert!(x > 0.0 && x < 800.0 && y > 0.0 && y < 600.0);
        let (x0, y0) = m.project(&nw);
        assert!(x0.abs() < 1.0 && y0.abs() < 1.0);
    }
}
