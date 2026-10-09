//! Streaming parser for repomix text output: `<file path="...">...</file>`.

use std::io::{BufRead, BufReader};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub content: String,
}

fn extract_path(line: &str) -> Option<String> {
    let line = line.trim();
    let rest = line.strip_prefix("<file path=\"")?;
    let end = rest.find("\">").or_else(|| rest.find('>'))?;
    Some(rest[..end].to_string())
}

/// Parse all file entries from a repomix pack. Streams line-by-line.
pub fn parse_pack(path: &str) -> std::io::Result<Vec<FileEntry>> {
    let f = std::fs::File::open(path)?;
    let mut entries = Vec::new();
    let mut cur: Option<FileEntry> = None;
    for line in BufReader::new(f).lines() {
        let line = line?;
        if let Some(p) = extract_path(&line) {
            if let Some(e) = cur.take() {
                entries.push(e);
            }
            cur = Some(FileEntry { path: p, content: String::new() });
        } else if line.trim() == "</file>" {
            if let Some(e) = cur.take() {
                entries.push(e);
            }
        } else if let Some(e) = cur.as_mut() {
            e.content.push_str(&line);
            e.content.push('\n');
        }
    }
    if let Some(e) = cur.take() {
        entries.push(e);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_pack() {
        let p = "/tmp/repomix_mine_snippet.txt";
        std::fs::write(
            p,
            "<file path=\"PALANTOUR/crates/pt-x/src/main.rs\">\nfn main() {}\n</file>\n<file path=\"README.md\">\nhi\n</file>\n",
        )
        .unwrap();
        let e = parse_pack(p).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].path, "PALANTOUR/crates/pt-x/src/main.rs");
        assert!(e[0].content.contains("fn main"));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn real_pack_file_count() {
        // Full parse is done by restore(); here just assert the marker count
        // matches the ledger-noted 3,834 files.
        let data = std::fs::read("/home/hatch/workspace/user/files/repomix-output_1__14_mh69.txt").unwrap();
        let s = String::from_utf8_lossy(&data);
        assert_eq!(s.matches("<file path=\"").count(), 3834);
    }
}
