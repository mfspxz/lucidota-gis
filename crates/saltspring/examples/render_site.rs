fn main() {
    let errs = saltspring::validate_all();
    assert!(errs.is_empty(), "{errs:?}");
    saltspring::render_site("/home/hatch/workspace/gis/sites/saltspring").unwrap();
    println!("site rendered");
}
