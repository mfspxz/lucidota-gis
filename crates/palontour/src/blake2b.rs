//! Pure-Rust BLAKE2b (RFC 7693). Verified against system b2sum.

const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

#[inline]
fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

fn compress(h: &mut [u64; 8], block: &[u8; 128], t: u128, last: bool) {
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..].copy_from_slice(&IV);
    v[12] ^= t as u64;
    v[13] ^= (t >> 64) as u64;
    if last {
        v[14] = !v[14];
    }
    let m = |i: usize| u64::from_le_bytes(block[8 * i..8 * i + 8].try_into().unwrap());
    for s in &SIGMA {
        g(&mut v, 0, 4, 8, 12, m(s[0]), m(s[1]));
        g(&mut v, 1, 5, 9, 13, m(s[2]), m(s[3]));
        g(&mut v, 2, 6, 10, 14, m(s[4]), m(s[5]));
        g(&mut v, 3, 7, 11, 15, m(s[6]), m(s[7]));
        g(&mut v, 0, 5, 10, 15, m(s[8]), m(s[9]));
        g(&mut v, 1, 6, 11, 12, m(s[10]), m(s[11]));
        g(&mut v, 2, 7, 8, 13, m(s[12]), m(s[13]));
        g(&mut v, 3, 4, 9, 14, m(s[14]), m(s[15]));
    }
    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

/// BLAKE2b with digest length nn (1..=64), unkeyed.
pub fn blake2b(data: &[u8], nn: usize) -> Vec<u8> {
    assert!((1..=64).contains(&nn), "digest length out of range");
    let mut h = IV;
    // param block: digest_length=nn, key_length=0, fanout=1, depth=1
    h[0] ^= 0x01010000 ^ (nn as u64);
    let mut t: u64 = 0;
    if data.is_empty() {
        compress(&mut h, &[0u8; 128], 0, true);
    } else {
        let mut chunks = data.chunks(128).peekable();
        while let Some(c) = chunks.next() {
            let last = chunks.peek().is_none();
            t = t.wrapping_add(c.len() as u64);
            let mut block = [0u8; 128];
            block[..c.len()].copy_from_slice(c);
            compress(&mut h, &block, t as u128, last);
        }
    }
    let mut out = Vec::with_capacity(nn);
    for w in h.iter().take(nn.div_ceil(8)) {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.truncate(nn);
    out
}

pub fn blake2b_512(data: &[u8]) -> [u8; 64] {
    blake2b(data, 64).try_into().unwrap()
}

pub fn blake2b_256(data: &[u8]) -> [u8; 32] {
    blake2b(data, 32).try_into().unwrap()
}

pub fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 15) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_512_matches_b2sum() {
        // ground truth: b2sum on this VM, 2026-10-06
        assert_eq!(
            hex(&blake2b_512(b"")),
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
        );
    }

    #[test]
    fn abc_512_matches_b2sum() {
        assert_eq!(
            hex(&blake2b_512(b"abc")),
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );
    }

    #[test]
    fn abc_256_matches_b2sum() {
        assert_eq!(
            hex(&blake2b_256(b"abc")),
            "bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319"
        );
    }

    #[test]
    fn multiblock_input_stable() {
        let data: Vec<u8> = (0..1000).map(|i| (i % 251) as u8).collect();
        let a = hex(&blake2b_512(&data));
        let b = hex(&blake2b_512(&data));
        assert_eq!(a, b);
        assert_eq!(a.len(), 128);
    }

    #[test]
    fn avalanche_on_single_bit() {
        let mut d = b"abc".to_vec();
        let a = hex(&blake2b_256(&d));
        d[0] ^= 1;
        let b = hex(&blake2b_256(&d));
        assert_ne!(a, b);
    }
}
