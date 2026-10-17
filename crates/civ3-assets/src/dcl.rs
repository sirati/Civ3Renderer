//! PKWare Data Compression Library "implode" streams, as Civilization III
//! compresses its scenarios (`.biq`) and saved games (`.sav`): a port of
//! Mark Adler's `blast.c` (zlib's contrib/blast), decoding into memory.

/// Longest Huffman code.
const MAX_BITS: usize = 13;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DclError {
    #[error("DCL stream: unknown literal mode {0}")]
    LiteralMode(u32),
    #[error("DCL stream: dictionary size {0} out of range")]
    Dictionary(u32),
    #[error("DCL stream: ended before its end code")]
    Truncated,
    #[error("DCL stream: copy from {distance} bytes back with only {have} written")]
    Distance { distance: usize, have: usize },
    #[error("DCL stream: invalid code")]
    Code,
}

/// A canonical Huffman code: how many codes of each length, and the
/// symbols in code order.
struct Huffman {
    count: [u16; MAX_BITS + 1],
    symbol: Vec<u16>,
}

impl Huffman {
    /// From the compact code lengths of the format: each byte gives a
    /// length (low 4 bits) and a repeat count minus one (high 4 bits).
    fn new(rep: &[u8]) -> Huffman {
        let lengths: Vec<usize> = rep.iter().flat_map(|&b| std::iter::repeat_n(usize::from(b & 15), usize::from(b >> 4) + 1)).collect();
        let mut count = [0u16; MAX_BITS + 1];
        for &l in &lengths {
            count[l] += 1;
        }
        let mut offs = [0usize; MAX_BITS + 1];
        for len in 1..MAX_BITS {
            offs[len + 1] = offs[len] + usize::from(count[len]);
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate().filter(|(_, l)| **l != 0) {
            symbol[offs[l]] = s as u16;
            offs[l] += 1;
        }
        Huffman { count, symbol }
    }
}

struct Bits<'a> {
    input: &'a [u8],
    at: usize,
    buf: u32,
    cnt: u32,
}

impl Bits<'_> {
    fn byte(&mut self) -> Result<u32, DclError> {
        let b = *self.input.get(self.at).ok_or(DclError::Truncated)?;
        self.at += 1;
        Ok(u32::from(b))
    }

    fn bits(&mut self, need: u32) -> Result<u32, DclError> {
        let mut val = self.buf;
        while self.cnt < need {
            val |= self.byte()? << self.cnt;
            self.cnt += 8;
        }
        self.buf = val >> need;
        self.cnt -= need;
        Ok(val & ((1 << need) - 1))
    }

    /// One symbol of `h` (the codes are stored bit-inverted, first bit
    /// first).
    fn decode(&mut self, h: &Huffman) -> Result<u16, DclError> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0usize);
        let mut buf = self.buf;
        let mut left = self.cnt;
        let mut len = 1usize;
        loop {
            while left > 0 {
                left -= 1;
                code |= ((buf & 1) ^ 1) as i32;
                buf >>= 1;
                let count = i32::from(h.count[len]);
                if code < first + count {
                    self.buf = buf;
                    self.cnt = (self.cnt.wrapping_sub(len as u32)) & 7;
                    return h.symbol.get(index + (code - first) as usize).copied().ok_or(DclError::Code);
                }
                index += count as usize;
                first = (first + count) << 1;
                code <<= 1;
                len += 1;
                if len > MAX_BITS {
                    return Err(DclError::Code);
                }
            }
            left = ((MAX_BITS + 1 - len) as u32).min(8);
            buf = self.byte()?;
        }
    }
}

/// Bit lengths of the literal, length and distance codes.
const LIT_LEN: [u8; 98] = [
    11, 124, 8, 7, 28, 7, 188, 13, 76, 4, 10, 8, 12, 10, 12, 10, 8, 23, 8, 9, 7, 6, 7, 8, 7, 6, 55, 8, 23, 24, 12, 11, 7, 9, 11, 12, 6,
    7, 22, 5, 7, 24, 6, 11, 9, 6, 7, 22, 7, 11, 38, 7, 9, 8, 25, 11, 8, 11, 9, 12, 8, 12, 5, 38, 5, 38, 5, 11, 7, 5, 6, 21, 6, 10, 53, 8,
    7, 24, 10, 27, 44, 253, 253, 253, 252, 252, 252, 13, 12, 45, 12, 45, 12, 61, 12, 45, 44, 173,
];
const LEN_LEN: [u8; 6] = [2, 35, 36, 53, 38, 23];
const DIST_LEN: [u8; 7] = [2, 20, 53, 230, 247, 151, 248];
/// Base and extra bits of the length codes.
const BASE: [u32; 16] = [3, 2, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 40, 72, 136, 264];
const EXTRA: [u32; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];

/// Decompresses a whole DCL stream.
pub fn explode(input: &[u8]) -> Result<Vec<u8>, DclError> {
    let lit_code = Huffman::new(&LIT_LEN);
    let len_code = Huffman::new(&LEN_LEN);
    let dist_code = Huffman::new(&DIST_LEN);
    let mut s = Bits { input, at: 0, buf: 0, cnt: 0 };
    let lit = s.bits(8)?;
    if lit > 1 {
        return Err(DclError::LiteralMode(lit));
    }
    let dict = s.bits(8)?;
    if !(4..=6).contains(&dict) {
        return Err(DclError::Dictionary(dict));
    }
    let mut out = Vec::with_capacity(input.len() * 4);
    loop {
        if s.bits(1)? != 0 {
            let symbol = usize::from(s.decode(&len_code)?);
            let len = BASE[symbol] + s.bits(EXTRA[symbol])?;
            if len == 519 {
                return Ok(out);
            }
            let extra = if len == 2 { 2 } else { dict };
            let distance = ((u32::from(s.decode(&dist_code)?) << extra) + s.bits(extra)? + 1) as usize;
            if distance > out.len() {
                return Err(DclError::Distance { distance, have: out.len() });
            }
            // (Overlapping copies repeat what they copy.)
            for _ in 0..len {
                out.push(out[out.len() - distance]);
            }
        } else {
            let symbol = if lit == 1 { u32::from(s.decode(&lit_code)?) } else { s.bits(8)? };
            out.push(symbol as u8);
        }
    }
}

/// Whether `data` looks like a DCL stream (rather than an uncompressed
/// file): literal mode 0 or 1, dictionary of 1, 2 or 4 KiB.
pub fn is_compressed(data: &[u8]) -> bool {
    matches!(data, [0 | 1, 4..=6, ..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_reference_example() {
        // blast.c's test vector: "AIAIAIAIAIAIA".
        let packed = [0x00, 0x04, 0x82, 0x24, 0x25, 0x8f, 0x80, 0x7f];
        assert_eq!(explode(&packed).unwrap(), b"AIAIAIAIAIAIA");
    }

    #[test]
    fn a_cut_stream_is_an_error() {
        let packed = [0x00, 0x04, 0x82, 0x24, 0x25];
        assert_eq!(explode(&packed), Err(DclError::Truncated));
        assert!(matches!(explode(&[7, 4, 0]), Err(DclError::LiteralMode(7))));
    }
}
