fn main() {
    use map_export::{SvgGeom, SvgMap};
    let r = coombs::region();
    let nw = geo_core::coord::LatLon::new_unchecked(r.max_lat, r.min_lon);
    let se = geo_core::coord::LatLon::new_unchecked(r.min_lat, r.max_lon);
    let mut m = SvgMap::new(1000, 700, nw, se, "Coombs / Parksville GIS");
    for pl in coombs::places() {
        m.add(SvgGeom::Point(pl.pos, pl.name.to_string()));
    }
    std::fs::write("/home/hatch/workspace/gis/sites/coombs.svg", m.render()).unwrap();
    println!("coombs.svg written");
}
