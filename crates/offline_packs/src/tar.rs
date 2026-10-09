//! Minimal tar writer/reader (ustar-ish, dependency-free). Enough for .gispack.

pub fn write_tar(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in files {
        let mut h = [0u8; 512];
        let nb = name.as_bytes();
        assert!(nb.len() < 100, "tar name too long");
        h[..nb.len()].copy_from_slice(nb);
        h[100..108].copy_from_slice(b"0000777\0");
        let sz = format!("{:011o}\0", data.len());
        h[124..136].copy_from_slice(sz.as_bytes());
        h[156] = b'0';
        h[257..262].copy_from_slice(b"ustar");
        // checksum with chksum field as spaces
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat(0u8).take(pad));
    }
    out.extend(std::iter::repeat(0u8).take(1024)); // two zero blocks
    out
}

#[derive(Debug)]
pub struct TarEntry {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn read_tar(buf: &[u8]) -> Result<Vec<TarEntry>, String> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        if pos + 512 > buf.len() {
            break;
        }
        let h = &buf[pos..pos + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let end = h[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let name = String::from_utf8_lossy(&h[..end]).into_owned();
        let szs = String::from_utf8_lossy(&h[124..136]);
        let size = usize::from_str_radix(szs.trim_matches(|c| c == '\0' || c == ' '), 8)
            .map_err(|_| "bad tar size")?;
        pos += 512;
        if pos + size > buf.len() {
            return Err("tar truncated".into());
        }
        out.push(TarEntry { name, data: buf[pos..pos + size].to_vec() });
        pos += size + (512 - size % 512) % 512;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tar_roundtrip() {
        let files = vec![
            ("a.txt".into(), b"hello".to_vec()),
            ("d/b.bin".into(), vec![1u8; 700]),
        ];
        let t = write_tar(&files);
        let back = read_tar(&t).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].data, b"hello");
        assert_eq!(back[1].data.len(), 700);
        // system tar can read it too (external interop check)
        std::fs::write("/tmp/gispack_tar_test.tar", &t).unwrap();
        let o = std::process::Command::new("tar").args(["tf", "/tmp/gispack_tar_test.tar"]).output().unwrap();
        assert!(o.status.success());
        let listing = String::from_utf8_lossy(&o.stdout);
        assert!(listing.contains("a.txt") && listing.contains("d/b.bin"));
    }
}
