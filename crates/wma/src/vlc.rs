//! Table-driven decoder for prefix codes given as explicit `(length, codeword)` pairs.

use crate::bits::BitReader;

const ROOT_BITS: u32 = 9;

#[derive(Clone, Copy)]
enum Entry {
    Invalid,
    /// Symbol and the number of bits it consumes at this level.
    Leaf {
        symbol: u16,
        len: u8,
    },
    /// Continue at `offset`, indexing with the next `bits` bits.
    Sub {
        offset: u32,
        bits: u8,
    },
}

pub struct Vlc {
    table: Vec<Entry>,
}

impl Vlc {
    /// Symbol `i` is coded by `codes[i]`.
    pub fn new(codes: &[(u8, u32)]) -> Self {
        let items: Vec<(u8, u32, u16)> = codes
            .iter()
            .enumerate()
            .map(|(sym, &(len, code))| (len, code, sym as u16))
            .collect();
        let mut table = Vec::new();
        fill(&mut table, &items, 0, ROOT_BITS);
        Vlc { table }
    }

    /// Decode one symbol, or `None` for a codeword outside the table.
    pub fn read(&self, br: &mut BitReader) -> Option<u16> {
        let mut base = 0usize;
        let mut bits = ROOT_BITS;
        loop {
            match self.table[base + br.peek(bits) as usize] {
                Entry::Leaf { symbol, len } => {
                    br.skip(u32::from(len));
                    return Some(symbol);
                }
                Entry::Sub { offset, bits: b } => {
                    br.skip(bits);
                    base = offset as usize;
                    bits = u32::from(b);
                }
                Entry::Invalid => {
                    br.skip(bits);
                    return None;
                }
            }
        }
    }
}

/// Lay out one lookup level for `items`, whose first `consumed` bits are already matched.
/// Returns the level's offset in `table`.
fn fill(table: &mut Vec<Entry>, items: &[(u8, u32, u16)], consumed: u32, bits: u32) -> u32 {
    let base = table.len();
    table.resize(base + (1 << bits), Entry::Invalid);
    let mut longer: Vec<(u32, (u8, u32, u16))> = Vec::new();
    for &item in items {
        let (len, code, symbol) = item;
        let rem = u32::from(len) - consumed;
        let tail = code & ((1u32 << rem) - 1);
        if rem <= bits {
            let start = (tail << (bits - rem)) as usize;
            let entry = Entry::Leaf {
                symbol,
                len: rem as u8,
            };
            table[base + start..base + start + (1 << (bits - rem))].fill(entry);
        } else {
            longer.push((tail >> (rem - bits), item));
        }
    }
    longer.sort_by_key(|&(prefix, _)| prefix);
    for group in longer.chunk_by(|a, b| a.0 == b.0) {
        let prefix = group[0].0 as usize;
        let max_len = group.iter().map(|g| u32::from(g.1 .0)).max().unwrap();
        let sub_bits = (max_len - consumed - bits).min(ROOT_BITS);
        let sub: Vec<_> = group.iter().map(|g| g.1).collect();
        let offset = fill(table, &sub, consumed + bits, sub_bits);
        table[base + prefix] = Entry::Sub {
            offset,
            bits: sub_bits as u8,
        };
    }
    base as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables;

    /// Write `symbols` with `codes`, then read them back.
    fn round_trip(codes: &[(u8, u32)]) {
        let mut bits: Vec<bool> = Vec::new();
        let symbols: Vec<usize> = (0..codes.len()).rev().chain(0..codes.len()).collect();
        for &s in &symbols {
            let (len, code) = codes[s];
            for i in (0..len).rev() {
                bits.push(code >> i & 1 == 1);
            }
        }
        let bytes: Vec<u8> = bits
            .chunks(8)
            .map(|c| {
                c.iter()
                    .enumerate()
                    .fold(0u8, |a, (i, &b)| a | (u8::from(b) << (7 - i)))
            })
            .collect();
        let vlc = Vlc::new(codes);
        let mut br = BitReader::new(&bytes);
        for &s in &symbols {
            assert_eq!(vlc.read(&mut br), Some(s as u16));
        }
        assert_eq!(br.pos(), bits.len());
    }

    #[test]
    fn all_tables_round_trip() {
        for codes in [
            &tables::COEF0_CODES[..],
            &tables::COEF1_CODES,
            &tables::COEF2_CODES,
            &tables::COEF3_CODES,
            &tables::COEF4_CODES,
            &tables::COEF5_CODES,
            &tables::EXP_CODES,
            &tables::HGAIN_CODES,
        ] {
            round_trip(codes);
        }
    }
}
