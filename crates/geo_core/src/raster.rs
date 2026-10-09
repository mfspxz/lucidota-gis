//! RGB raster + PNG writer (stored-deflate, dependency-free) + PPM P6 reader.

use std::io::Write;

/// Row-major RGB raster.
#[derive(Debug, Clone)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGB triples
}

impl Raster {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; width as usize * height as usize * 3],
        }
    }

    pub fn filled(width: u32, height: u32, r: u8, g: u8, b: u8) -> Self {
        let mut v = Vec::with_capacity(width as usize * height as usize * 3);
        for _ in 0..(width as usize * height as usize) {
            v.extend_from_slice(&[r, g, b]);
        }
        Self { width, height, data: v }
    }

    #[inline]
    pub fn set_px(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 3;
        self.data[i..i + 3].copy_from_slice(&rgb);
    }

    #[inline]
    pub fn get_px(&self, x: u32, y: u32) -> [u8; 3] {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Blit another raster at offset.
    pub fn blit(&mut self, src: &Raster, dx: i64, dy: i64) {
        for y in 0..src.height as i64 {
            for x in 0..src.width as i64 {
                let tx = dx + x;
                let ty = dy + y;
                if tx >= 0 && ty >= 0 && tx < self.width as i64 && ty < self.height as i64 {
                    self.set_px(tx as u32, ty as u32, src.get_px(x as u32, y as u32));
                }
            }
        }
    }

    /// Nearest-neighbour downscale by 2 (for tile pyramids).
    pub fn downscale_half(&self) -> Raster {
        let w = (self.width / 2).max(1);
        let h = (self.height / 2).max(1);
        let mut out = Raster::new(w, h);
        for y in 0..h {
            for x in 0..w {
                out.set_px(x, y, self.get_px(x * 2, y * 2));
            }
        }
        out
    }

    /// Raw zlib stream of the scanlines (stored deflate). Shared by PNG writer
    /// and PDF FlateDecode embedding (map_export).
    pub fn zlib_stream(&self) -> Vec<u8> {
        let mut z: Vec<u8> = vec![0x78, 0x01];
        let row = self.width as usize * 3;
        let mut scan: Vec<u8> = Vec::with_capacity((row + 1) * self.height as usize);
        for y in 0..self.height as usize {
            scan.push(0);
            scan.extend_from_slice(&self.data[y * row..(y + 1) * row]);
        }
        let mut adler = (1u32, 0u32);
        for &b in &scan {
            adler.1 = (adler.1 + adler.0 + b as u32) % 65521;
            adler.0 = (adler.0 + b as u32) % 65521;
        }
        let mut rest = scan.as_slice();
        while !rest.is_empty() {
            let n = rest.len().min(65_535);
            let last = rest.len() == n;
            z.push(if last { 0x01 } else { 0x00 });
            z.extend_from_slice(&(n as u16).to_le_bytes());
            z.extend_from_slice(&(!(n as u16)).to_le_bytes());
            z.extend_from_slice(&rest[..n]);
            rest = &rest[n..];
        }
        z.extend_from_slice(&((adler.1 << 16) | adler.0).to_be_bytes());
        z
    }

    /// Write a valid PNG (8-bit RGB, stored deflate blocks — no compression, universally readable).
    pub fn write_png<W: Write>(&self, mut w: W) -> std::io::Result<()> {
        fn crc32(buf: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &b in buf {
                crc ^= b as u32;
                for _ in 0..8 {
                    crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            !crc
        }
        fn chunk<W: Write>(w: &mut W, name: &[u8; 4], data: &[u8]) -> std::io::Result<()> {
            w.write_all(&(data.len() as u32).to_be_bytes())?;
            w.write_all(name)?;
            w.write_all(data)?;
            let mut c = Vec::new();
            c.extend_from_slice(name);
            c.extend_from_slice(data);
            w.write_all(&crc32(&c).to_be_bytes())?;
            Ok(())
        }
        w.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])?;
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&self.width.to_be_bytes());
        ihdr.extend_from_slice(&self.height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, RGB, deflate, filter 0, no interlace
        chunk(&mut w, b"IHDR", &ihdr)?;

        let idat = self.zlib_stream();
        chunk(&mut w, b"IDAT", &idat)?;
        chunk(&mut w, b"IEND", &[])?;
        Ok(())
    }

    /// Read binary PPM (P6). Our documented drone-ingest format: no external deps.
    pub fn read_ppm(path: &str) -> std::io::Result<Raster> {
        let bytes = std::fs::read(path)?;
        let mut pos = 0;
        let mut tok = || -> std::io::Result<Vec<u8>> {
            loop {
                while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\n' || bytes[pos] == b'\r' || bytes[pos] == b'\t') {
                    pos += 1;
                }
                if pos < bytes.len() && bytes[pos] == b'#' {
                    while pos < bytes.len() && bytes[pos] != b'\n' {
                        pos += 1;
                    }
                    continue;
                }
                break;
            }
            let start = pos;
            while pos < bytes.len() && !matches!(bytes[pos], b' ' | b'\n' | b'\r' | b'\t') {
                pos += 1;
            }
            Ok(bytes[start..pos].to_vec())
        };
        let magic = tok()?;
        if magic != b"P6" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not P6 PPM"));
        }
        let w: usize = String::from_utf8_lossy(&tok()?).parse().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad width")
        })?;
        let h: usize = String::from_utf8_lossy(&tok()?).parse().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad height")
        })?;
        let maxv: usize = String::from_utf8_lossy(&tok()?).parse().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad maxval")
        })?;
        if maxv != 255 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "maxval must be 255"));
        }
        pos += 1; // single whitespace after maxval
        if bytes.len() < pos + w * h * 3 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "truncated raster"));
        }
        Ok(Raster {
            width: w as u32,
            height: h as u32,
            data: bytes[pos..pos + w * h * 3].to_vec(),
        })
    }

    pub fn write_ppm(&self, path: &str) -> std::io::Result<()> {
        let mut f = std::fs::File::create(path)?;
        f.write_all(format!("P6\n{} {}\n255\n", self.width, self.height).as_bytes())?;
        f.write_all(&self.data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_roundtrip_via_header() {
        let r = Raster::filled(16, 16, 200, 100, 50);
        let mut buf = Vec::new();
        r.write_png(&mut buf).unwrap();
        // PNG signature + IHDR width/height present.
        assert_eq!(&buf[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert_eq!(&buf[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(buf[16..20].try_into().unwrap()), 16);
        assert_eq!(u32::from_be_bytes(buf[20..24].try_into().unwrap()), 16);
        assert_eq!(&buf[buf.len() - 8..buf.len() - 4], b"IEND");
    }

    #[test]
    fn ppm_roundtrip() {
        let mut r = Raster::new(4, 3);
        r.set_px(1, 1, [9, 8, 7]);
        let p = "/tmp/geo_core_ppm_test.ppm";
        r.write_ppm(p).unwrap();
        let q = Raster::read_ppm(p).unwrap();
        assert_eq!(q.width, 4);
        assert_eq!(q.get_px(1, 1), [9, 8, 7]);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn downscale_half_dims() {
        let r = Raster::filled(256, 256, 1, 2, 3);
        let d = r.downscale_half();
        assert_eq!((d.width, d.height), (128, 128));
    }
}
