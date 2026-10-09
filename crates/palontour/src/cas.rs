//! Content-addressed store: objects/<xx>/<rest> keyed by blake2b-256 hex.

use crate::blake2b::{blake2b_256, hex};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    pub fn open(root: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(root.join("objects"))?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    fn path_for(&self, digest: &str) -> PathBuf {
        self.root
            .join("objects")
            .join(&digest[..2])
            .join(&digest[2..])
    }

    /// Store bytes; returns the blake2b-256 hex digest. Idempotent.
    pub fn put(&self, data: &[u8]) -> std::io::Result<String> {
        let d = hex(&blake2b_256(data));
        let p = self.path_for(&d);
        if !p.exists() {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // write-then-rename for crash safety
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, data)?;
            std::fs::rename(&tmp, &p)?;
        }
        Ok(d)
    }

    pub fn get(&self, digest: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.path_for(digest))
    }

    pub fn exists(&self, digest: &str) -> bool {
        self.path_for(digest).exists()
    }

    /// Verify bytes match a claimed digest (anti-tamper on ingest).
    pub fn verify(&self, digest: &str, data: &[u8]) -> bool {
        hex(&blake2b_256(data)) == digest
    }

    pub fn import_file(&self, path: &Path) -> std::io::Result<String> {
        self.put(&std::fs::read(path)?)
    }

    /// Number of stored objects (walks objects/ once).
    pub fn object_count(&self) -> usize {
        let mut n = 0;
        if let Ok(top) = std::fs::read_dir(self.root.join("objects")) {
            for e1 in top.flatten() {
                if let Ok(sub) = std::fs::read_dir(e1.path()) {
                    n += sub.count();
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store(tag: &str) -> ContentStore {
        let p = std::env::temp_dir().join(format!("palontour_cas_{tag}"));
        let _ = std::fs::remove_dir_all(&p);
        ContentStore::open(&p).unwrap()
    }

    #[test]
    fn put_get_roundtrip() {
        let s = tmp_store("rt");
        let d = s.put(b"tile-bytes").unwrap();
        assert_eq!(s.get(&d).unwrap(), b"tile-bytes");
    }

    #[test]
    fn put_is_idempotent() {
        let s = tmp_store("idem");
        let a = s.put(b"x").unwrap();
        let b = s.put(b"x").unwrap();
        assert_eq!(a, b);
        assert_eq!(s.object_count(), 1);
    }

    #[test]
    fn verify_detects_tamper() {
        let s = tmp_store("tamper");
        let d = s.put(b"honest").unwrap();
        assert!(s.verify(&d, b"honest"));
        assert!(!s.verify(&d, b"forged"));
    }
}
