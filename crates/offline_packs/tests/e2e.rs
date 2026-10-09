//! End-to-end: synthetic drone frames -> ortho mosaic -> tile pyramid ->
//! .gispack -> palontour CAS + tileset seal. Leaves real artifacts in
//! ~/workspace/gis/{packs,cas}.

use geo_core::coord::LatLon;
use geo_core::raster::Raster;
use ortho::{build_pyramid, mosaic_frames, Frame};
use palontour::{ContentStore, Tileset};
use std::path::Path;

fn survey_frame(path: &str, nw: LatLon, seed: u8) -> Frame {
    // synthetic "survey" pattern: green base, grid lines, seed-tinted blocks
    let mut r = Raster::filled(128, 128, 34, 120, 50);
    for y in 0..128 {
        for x in 0..128 {
            if x % 16 == 0 || y % 16 == 0 {
                r.set_px(x, y, [20, 70, 30]);
            }
            if (x / 32 + y / 32) % 2 == 0 {
                let p = r.get_px(x, y);
                r.set_px(x, y, [p[0].saturating_add(seed), p[1], p[2]]);
            }
        }
    }
    r.write_ppm(path).unwrap();
    Frame { path: path.into(), nw, pixel_size_m: 8.0 }
}

#[test]
fn e2e_saltspring_demo_pack() {
    let base = "/home/hatch/workspace/gis";
    let frames = vec![
        survey_frame("/tmp/e2e_f1.ppm", LatLon::new_unchecked(48.8700, -123.5200), 40),
        survey_frame("/tmp/e2e_f2.ppm", LatLon::new_unchecked(48.8600, -123.5000), 80),
        survey_frame("/tmp/e2e_f3.ppm", LatLon::new_unchecked(48.7800, -123.4700), 120),
    ];
    let mosaic = mosaic_frames(&frames, 12).unwrap();
    let tiles = build_pyramid(&mosaic, 10, 12);
    assert!(!tiles.is_empty());

    // pack it
    let mut pb = offline_packs::PackBuilder::new(
        "saltspring-demo-z10-12",
        "{\"region\":\"saltspring-island\",\"demo\":true,\"note\":\"synthetic survey frames\"}",
    );
    for t in &tiles {
        pb.add_tile(t.tile.z, t.tile.x, t.tile.y, t.png.clone());
    }
    for (name, gj) in saltspring::layers_geojson() {
        pb.add_layer(&name, &gj);
    }
    std::fs::create_dir_all(format!("{base}/packs")).unwrap();
    let pack_path = format!("{base}/packs/saltspring-demo-z10-12.gispack");
    let manifest_digest = pb.build(&pack_path).unwrap();

    // seal tiles into CAS + tileset index
    let store = ContentStore::open(Path::new(&format!("{base}/cas"))).unwrap();
    let mut ts = Tileset::new("saltspring-demo-z10-12");
    for t in &tiles {
        let d = store.put(&t.png).unwrap();
        ts.insert(t.tile.z, t.tile.x, t.tile.y, d);
    }
    let index_digest = ts.seal(&store).unwrap();
    std::fs::write(
        format!("{base}/packs/tileset.json"),
        format!("{{\"name\":\"saltspring-demo-z10-12\",\"tiles\":{},\"root\":\"{}\",\"manifest\":\"{}\",\"index\":\"{}\"}}",
            ts.len(), ts.root_digest(), manifest_digest, index_digest),
    ).unwrap();

    // verify the pack reads back clean
    let info = offline_packs::PackReader::open(&pack_path).unwrap();
    assert_eq!(offline_packs::PackReader::verify(&info).unwrap(), tiles.len());
    assert!(info.layers.contains(&"places".to_string()));
}
