//! A digest of the game state: what players compare to find out that their games differ.

/// One digest: after `update` updates (each one 1/60 s at speed 10, so ten simulation
/// iterations), the year, the time in the year in ticks, the living citizens, and the CRC-32 of
/// the encoded game state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Digest {
    pub update: u32,
    pub year: u32,
    pub elapsed: i64,
    pub citizens: usize,
    pub crc: u32,
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "update {:>6}  year {} tick {:>10}  citizens {:>4}  state {:08x}",
            self.update, self.year, self.elapsed, self.citizens, self.crc
        )
    }
}
