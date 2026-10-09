//! Minimal PDF writer: one raster image per page (FlateDecode), title text,
//! page-size presets. xref offsets computed honestly — tests re-parse them.

use geo_core::raster::Raster;

#[derive(Debug, Clone, Copy)]
pub enum PageSize {
    A4Landscape,
    A4Portrait,
    LetterLandscape,
    LetterPortrait,
}

impl PageSize {
    pub fn pts(&self) -> (f64, f64) {
        match self {
            PageSize::A4Landscape => (841.89, 595.28),
            PageSize::A4Portrait => (595.28, 841.89),
            PageSize::LetterLandscape => (792.0, 612.0),
            PageSize::LetterPortrait => (612.0, 792.0),
        }
    }
}

fn esc_pdf(s: &str) -> String {
    s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

/// Render a raster + title as a single-page PDF.
pub fn raster_to_pdf(raster: &Raster, title: &str, size: PageSize) -> Vec<u8> {
    let (pw, ph) = size.pts();
    let img = raster.zlib_stream();
    let iw = raster.width;
    let ih = raster.height;

    // Fit image into page with 36pt margins, keep aspect.
    let max_w = pw - 72.0;
    let max_h = ph - 120.0;
    let scale = (max_w / iw as f64).min(max_h / ih as f64);
    let dw = iw as f64 * scale;
    let dh = ih as f64 * scale;
    let dx = (pw - dw) / 2.0;
    let dy = 60.0;

    let content = format!(
        "BT /F1 18 Tf 36 {} Td ({}) Tj ET\nq {} 0 0 {} {} {} cm /Im0 Do Q\n",
        ph - 40.0, esc_pdf(title), dw, dh, dx, dy
    );

    let img_obj = format!(
        "<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        img.len()
    );

    let mut pdf: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let mut obj = |pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, n: u32, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut pdf, &mut offsets, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut pdf, &mut offsets, 2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(&mut pdf, &mut offsets, 3, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 5 0 R >> /Font << /F1 6 0 R >> >> /Contents 4 0 R >>").as_bytes());
    obj(&mut pdf, &mut offsets, 4, &{
        let mut b = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        b.extend_from_slice(content.as_bytes());
        b.extend_from_slice(b"\nendstream");
        b
    });
    obj(&mut pdf, &mut offsets, 5, &{
        let mut b = img_obj.into_bytes();
        b.extend_from_slice(&img);
        b.extend_from_slice(b"\nendstream");
        b
    });
    obj(&mut pdf, &mut offsets, 6, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");

    let xref_pos = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for o in &offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n", offsets.len() + 1).as_bytes(),
    );
    pdf
}

/// Validate xref offsets: every listed offset must point at "N 0 obj".
pub fn validate_xref(pdf: &[u8]) -> Result<usize, String> {
    let s = String::from_utf8_lossy(pdf);
    let sx = s.rfind("startxref").ok_or("no startxref")?;
    let xref_pos: usize = s[sx..].lines().nth(1).ok_or("no xref pos")?.trim().parse().map_err(|_| "bad xref pos")?;
    if &pdf[xref_pos..xref_pos + 4] != b"xref" {
        return Err("xref offset does not point at xref".into());
    }
    let mut count = 0;
    for line in s[xref_pos..].lines().skip(2) {
        if line.starts_with("trailer") { break; }
        if line.len() >= 18 && line.as_bytes()[17] == b'n' {
            let off: usize = line[..10].parse().map_err(|_| "bad offset")?;
            let head = String::from_utf8_lossy(&pdf[off..off + 12]);
            if !head.contains(" 0 obj") {
                return Err(format!("offset {off} does not point at an object: {head}"));
            }
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_structure_valid() {
        let r = Raster::filled(64, 48, 20, 40, 80);
        let pdf = raster_to_pdf(&r, "Saltspring test map", PageSize::A4Landscape);
        assert_eq!(&pdf[0..5], b"%PDF-");
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/FlateDecode"));
        assert!(s.contains("(Saltspring test map)"));
        assert!(s.contains("/MediaBox [0 0 841.89 595.28]"));
        let n = validate_xref(&pdf).unwrap();
        assert_eq!(n, 6, "six objects expected");
    }

    #[test]
    fn pdf_image_dims_match_raster() {
        let r = Raster::filled(100, 50, 1, 2, 3);
        let pdf = raster_to_pdf(&r, "t", PageSize::LetterPortrait);
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Width 100 /Height 50"));
    }
}
