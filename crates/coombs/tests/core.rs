use coombs::{layers_geojson, offline_manifest, places, validate_all};

#[test]
fn coombs_validates_clean() {
    let errs = validate_all();
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn market_present() {
    assert!(places().iter().any(|p| p.name.contains("Goats") || p.name.contains("Market")));
}

#[test]
fn offline_manifest_shape() {
    let m = offline_manifest();
    assert!(m.contains("\"offline_capable\":true"));
    assert!(m.contains("coombs-parksville"));
    let (name, gj) = &layers_geojson()[0];
    assert_eq!(name, "coombs_places");
    assert!(gj.contains("Coombs Old Country Market"));
}
