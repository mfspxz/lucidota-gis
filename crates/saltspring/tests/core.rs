use saltspring::{ferry_routes, layers_geojson, places, region, render_site, validate_all};

#[test]
fn all_places_inside_region() {
    let errs = validate_all();
    assert!(errs.is_empty(), "validation errors: {errs:?}");
}

#[test]
fn dataset_has_expected_shape() {
    assert!(places().len() >= 12);
    assert_eq!(ferry_routes().len(), 3);
    let r = region();
    assert_eq!(r.name, "saltspring-island");
}

#[test]
fn layers_produce_valid_geojson() {
    let layers = layers_geojson();
    assert_eq!(layers.len(), 3);
    for (name, gj) in &layers {
        assert!(gj.starts_with("{\"type\":\"FeatureCollection\""), "layer {name}");
        assert!(gj.contains("\"features\""), "layer {name}");
    }
}

#[test]
fn site_renders() {
    let d = "/tmp/saltspring_site_test";
    let _ = std::fs::remove_dir_all(d);
    render_site(d).unwrap();
    let html = std::fs::read_to_string(format!("{d}/index.html")).unwrap();
    assert!(html.contains("Ganges"));
    assert!(html.contains("leaflet"));
    assert!(std::path::Path::new(&format!("{d}/data/places.geojson")).exists());
}
