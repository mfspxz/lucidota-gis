//! Restoration: mine the 32MB repomix pack, write inventory artifacts, and
//! seal them into the palontour CAS with a signed-by-construction receipt.

use crate::inventory::{build_inventory, gis_relevant_crates};
use crate::parse::parse_pack;
use palontour::ContentStore;
use std::io::Write;
use std::path::Path;

pub const PACK_PATH: &str = "/home/hatch/workspace/user/files/repomix-output_1__14_mh69.txt";

#[derive(Debug)]
pub struct RestoreReport {
    pub total_files: usize,
    pub palantour_crates: usize,
    pub palantour_files: usize,
    pub palantour_loc: usize,
    pub gis_relevant: Vec<String>,
    pub inventory_digest: String,
    pub out_dir: String,
}

/// Full restore run: parse -> inventory -> artifacts -> CAS seal.
pub fn restore(out_dir: &str, cas_dir: &str) -> std::io::Result<RestoreReport> {
    let entries = parse_pack(PACK_PATH)?;
    let inv = build_inventory(&entries);
    let pal_files: usize = inv.values().map(|c| c.rs_files.len() + c.other_files.len()).sum();
    let pal_loc: usize = inv.values().map(|c| c.total_loc).sum();
    let gis: Vec<String> = gis_relevant_crates(&inv).iter().map(|c| c.name.clone()).collect();

    std::fs::create_dir_all(out_dir)?;
    std::fs::create_dir_all(format!("{out_dir}/gis-relevant"))?;

    // crates.json (machine-readable)
    let mut jc = String::from("{\"crates\":[");
    let mut first = true;
    for c in inv.values() {
        if !first { jc.push(','); }
        first = false;
        let sigs: Vec<String> = c.gis_signals.iter().map(|s| format!("\"{s}\"")).collect();
        jc.push_str(&format!(
            "{{\"name\":\"{}\",\"rs_files\":{},\"other_files\":{},\"loc\":{},\"has_main\":{},\"has_lib\":{},\"gis_relevant\":{},\"signals\":[{}]}}",
            c.name, c.rs_files.len(), c.other_files.len(), c.total_loc,
            c.has_main, c.has_lib, c.gis_relevant, sigs.join(",")
        ));
    }
    jc.push_str("]}");
    std::fs::write(format!("{out_dir}/crates.json"), &jc)?;

    // PALANTOUR-INVENTORY.md (human-readable)
    let mut md = String::from("# PALONTOUR crate inventory (restored from repomix pack)\n\n");
    md.push_str(&format!("- pack: 32.5MB, {} files\n- PALONTOUR crates: {}\n- PALONTOUR files: {}, LOC: {}\n- GIS-relevant crates: {}\n\n",
        entries.len(), inv.len(), pal_files, pal_loc, gis.len()));
    md.push_str("| crate | rs | other | loc | main | lib | gis | signals |\n|---|---|---|---|---|---|---|---|\n");
    for c in inv.values() {
        md.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            c.name, c.rs_files.len(), c.other_files.len(), c.total_loc,
            c.has_main, c.has_lib, if c.gis_relevant { "YES" } else { "" },
            c.gis_signals.join("; ")));
    }
    std::fs::write(format!("{out_dir}/PALANTOUR-INVENTORY.md"), &md)?;

    // gis-relevant: dump the actual source of GIS-relevant crates for module restoration
    let gis_set: std::collections::HashSet<&str> = gis.iter().map(|s| s.as_str()).collect();
    let mut restored_modules = 0;
    for e in &entries {
        if let Some(rest) = e.path.strip_prefix("PALANTOUR/crates/") {
            let cname = rest.split('/').next().unwrap_or("");
            if gis_set.contains(cname) && e.path.ends_with(".rs") {
                let dest = format!("{out_dir}/gis-relevant/{rest}");
                if let Some(parent) = Path::new(&dest).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut f = std::fs::File::create(&dest)?;
                f.write_all(e.content.as_bytes())?;
                restored_modules += 1;
            }
        }
    }

    // Seal crates.json into the CAS; the digest IS the restoration receipt.
    let store = ContentStore::open(Path::new(cas_dir))?;
    let digest = store.put(jc.as_bytes())?;
    let receipt = format!(
        "{{\"pack\":\"repomix-output_1__14_mh69.txt\",\"files\":{},\"crates\":{},\"gis_relevant\":{},\"restored_rs_modules\":{},\"inventory_digest\":\"{}\",\"cas_dir\":\"{}\"}}",
        entries.len(), inv.len(), gis.len(), restored_modules, digest, cas_dir
    );
    std::fs::write(format!("{out_dir}/RESTORE-RECEIPT.json"), &receipt)?;

    Ok(RestoreReport {
        total_files: entries.len(),
        palantour_crates: inv.len(),
        palantour_files: pal_files,
        palantour_loc: pal_loc,
        gis_relevant: gis,
        inventory_digest: digest,
        out_dir: out_dir.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_runs_end_to_end() {
        let out = "/tmp/repomix_mine_restore_test";
        let cas = "/tmp/repomix_mine_cas_test";
        let _ = std::fs::remove_dir_all(out);
        let _ = std::fs::remove_dir_all(cas);
        let r = restore(out, cas).unwrap();
        assert_eq!(r.total_files, 3834);
        assert!(r.palantour_crates >= 60, "crates={}", r.palantour_crates);
        assert!(!r.gis_relevant.is_empty());
        assert!(r.gis_relevant.iter().any(|n| n.contains("odin")),
            "gis crates: {:?}", r.gis_relevant);
        assert!(Path::new(&format!("{out}/crates.json")).exists());
        assert!(Path::new(&format!("{out}/PALANTOUR-INVENTORY.md")).exists());
        assert!(Path::new(&format!("{out}/RESTORE-RECEIPT.json")).exists());
        assert_eq!(r.inventory_digest.len(), 64); // blake2b-256 hex
        // CAS actually holds the inventory
        let store = ContentStore::open(Path::new(cas)).unwrap();
        assert!(store.exists(&r.inventory_digest));
    }
}
