//! gis_docs — gis-018: canon docs + mechanical doc tests.
//!
//! Docs live in docs/. Tests enforce: all six canon files exist, no broken
//! relative links, fenced code blocks carry language tags.

#[cfg(test)]
mod tests {
    use std::path::Path;

    const DOCS: &[&str] = &[
        "ARCHITECTURE.md",
        "DATA-DICTIONARY.md",
        "INGEST.md",
        "OFFLINE-PACKS.md",
        "API.md",
        "REDTEAM.md",
    ];

    fn docs_dir() -> String {
        let m = env!("CARGO_MANIFEST_DIR");
        format!("{m}/docs")
    }

    #[test]
    fn all_canon_docs_exist() {
        for d in DOCS {
            let p = format!("{}/{}", docs_dir(), d);
            assert!(Path::new(&p).exists(), "missing {d}");
            let s = std::fs::read_to_string(&p).unwrap();
            assert!(s.len() > 200, "{d} too thin");
        }
    }

    #[test]
    fn no_broken_relative_links() {
        for d in DOCS {
            let s = std::fs::read_to_string(format!("{}/{}", docs_dir(), d)).unwrap();
            // crude link scan: ](target) where target looks relative
            let mut rest = s.as_str();
            while let Some(i) = rest.find("](") {
                rest = &rest[i + 2..];
                if let Some(end) = rest.find(')') {
                    let target = &rest[..end];
                    if !target.is_empty()
                        && !target.starts_with("http")
                        && !target.starts_with('#')
                        && !target.starts_with("mailto:")
                    {
                        let p = format!("{}/{}", docs_dir(), target);
                        assert!(Path::new(&p).exists(), "{d}: broken link {target}");
                    }
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        }
    }

    #[test]
    fn code_fences_tagged() {
        for d in DOCS {
            let s = std::fs::read_to_string(format!("{}/{}", docs_dir(), d)).unwrap();
            let mut in_fence = false;
            for line in s.lines() {
                let t = line.trim();
                if t.starts_with("```") {
                    if !in_fence && t == "```" {
                        panic!("{d}: untagged opening code fence");
                    }
                    in_fence = !in_fence;
                }
            }
        }
    }
}
