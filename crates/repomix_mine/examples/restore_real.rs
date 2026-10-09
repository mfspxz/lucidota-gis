fn main() {
    let r = repomix_mine::restore(
        "/home/hatch/workspace/gis/restored",
        "/home/hatch/workspace/gis/cas",
    ).unwrap();
    println!("files={} crates={} pal_files={} pal_loc={} gis_relevant={:?}",
        r.total_files, r.palantour_crates, r.palantour_files, r.palantour_loc, r.gis_relevant);
    println!("inventory_digest={}", r.inventory_digest);
}
