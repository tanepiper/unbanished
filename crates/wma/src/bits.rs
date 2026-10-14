//! MSB-first bit reader. Reads past the end of the buffer yield zero bits, matching how WMA
//! frames are allowed to run into packet padding.

pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0 }
    }

    /// Current position in bits from the start of the buffer.
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len_bits(&self) -> usize {
        self.data.len() * 8
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn skip(&mut self, n: u32) {
        self.pos += n as usize;
    }

    /// The next `n` (<= 32) bits without consuming them.
    pub fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        let byte = self.pos >> 3;
        let mut window = 0u64;
        for i in 0..8 {
            window = (window << 8) | u64::from(self.data.get(byte + i).copied().unwrap_or(0));
        }
        let shift = 64 - (self.pos & 7) as u32 - n;
        ((window >> shift) & ((1u64 << n) - 1)) as u32
    }

    pub fn bits(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.skip(n);
        v
    }

    pub fn bit(&mut self) -> bool {
        self.bits(1) != 0
    }
}
