//! Town storage and the stock table: the New Game starting goods (`FUN_1400a73d0` →
//! `FUN_140096b30`) and the per-iteration category totals the Status Bar shows
//! (`FUN_14008f3a0`). See docs/STARTING-INVENTORY-DISTRIBUTION.md.

/// A raw material's storage properties (`RawMaterialDescription`).
#[derive(Debug, Clone)]
pub struct RawMaterial {
    /// `Template\RawMaterialX.rsc:resource`.
    pub reference: String,
    /// `+0x48`: storage units per item.
    pub size: i32,
    /// `+0x5c`: category flags.
    pub flags: u32,
}

/// A storage component (`StorageComponent`): capacity `+0x38`, accepted flags `+0x3c`, and its
/// `(raw material, count)` list `+0x28`.
#[derive(Debug, Clone)]
pub struct Storage {
    pub capacity: i32,
    pub accepts: u32,
    pub items: Vec<(usize, i32)>,
}

impl Storage {
    /// `FUN_1401323e0`: units in use.
    pub fn used(&self, materials: &[RawMaterial]) -> i32 {
        self.items.iter().map(|&(m, n)| materials[m].size * n).sum()
    }

    /// `FUN_140132250`: accepted (`FUN_140131a30`) and `size · count` fits.
    pub fn fits(&self, materials: &[RawMaterial], material: usize, count: i32) -> bool {
        if self.accepts & materials[material].flags == 0 {
            return false;
        }
        materials[material].size * count + self.used(materials) <= self.capacity
    }

    /// `FUN_1401324d0`: add to the material's entry. The original adds to an existing entry and
    /// then also appends a new `(material, count)` entry; the totals it sums are the same.
    pub fn add(&mut self, material: usize, count: i32) {
        self.items.push((material, count));
    }
}

/// `FUN_140096b30`: hand out up to `cap` single units of the candidates matching `mask`,
/// round-robin over the candidates and one unit per storage per pass, until the cap is reached
/// or a pass adds nothing.
pub fn distribute(
    storages: &mut [Storage],
    materials: &[RawMaterial],
    candidates: &[usize],
    mask: u32,
    cap: i32,
) {
    let list: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|&c| materials[c].flags & mask != 0)
        .collect();
    if list.is_empty() || cap <= 0 {
        return;
    }
    let mut given = 0;
    loop {
        let mut progressed = false;
        for s in storages.iter_mut() {
            if given < cap {
                let m = list[given as usize % list.len()];
                if s.fits(materials, m, 1) {
                    s.add(m, 1);
                    given += 1;
                    progressed = true;
                }
            }
        }
        if !progressed || cap <= given {
            break;
        }
    }
}

/// The Status Bar stock slots (`Game +0xf0 → +0x170`, 0x20 bytes each): wood, stone, iron,
/// firewood, tools, food, herbs, clothes, alcohol, coal, textile, custom 0..9.
pub const STOCK_SLOTS: usize = 21;

/// `FUN_14008f3a0`: which slot a raw material counts towards, in the original's test order
/// (food, then firewood, then single flag bits).
pub fn stock_slot(flags: u32) -> Option<usize> {
    if flags & 0x20 != 0 {
        return Some(5);
    }
    if flags & 0x80 != 0 {
        return Some(3);
    }
    const BITS: [(u32, usize); 18] = [
        (8, 9),
        (9, 4),
        (10, 0),
        (11, 1),
        (12, 2),
        (13, 6),
        (14, 7),
        (17, 8),
        (16, 10),
        (18, 11),
        (19, 12),
        (20, 13),
        (21, 14),
        (22, 15),
        (23, 16),
        (24, 17),
        (25, 18),
        (26, 19),
    ];
    for (bit, slot) in BITS {
        if flags >> bit & 1 != 0 {
            return Some(slot);
        }
    }
    if flags >> 27 & 1 != 0 {
        return Some(20);
    }
    None
}

/// Recompute the stock table from every storage.
pub fn stock_table(storages: &[Storage], materials: &[RawMaterial]) -> [i32; STOCK_SLOTS] {
    let mut table = [0; STOCK_SLOTS];
    for s in storages {
        for &(m, n) in &s.items {
            if let Some(slot) = stock_slot(materials[m].flags) {
                table[slot] += n;
            }
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material(size: i32, flags: u32) -> RawMaterial {
        RawMaterial {
            reference: String::new(),
            size,
            flags,
        }
    }

    #[test]
    fn slots() {
        assert_eq!(stock_slot(0x400), Some(0)); // wood
        assert_eq!(stock_slot(0x800), Some(1)); // stone
        assert_eq!(stock_slot(0x1000), Some(2)); // iron
        assert_eq!(stock_slot(0xc0), Some(3)); // firewood
        assert_eq!(stock_slot(0x200), Some(4)); // tools
        assert_eq!(stock_slot(0x22), Some(5)); // potato
        assert_eq!(stock_slot(0x4000), Some(7)); // leather coat
    }

    #[test]
    fn round_robin_respects_capacity_and_flags() {
        let materials = vec![material(11, 0x400), material(1, 0x22)];
        let mut storages = vec![
            Storage {
                capacity: 6000,
                accepts: 0x16220,
                items: vec![],
            },
            Storage {
                capacity: 50,
                accepts: 0x1c40,
                items: vec![],
            },
        ];
        distribute(&mut storages, &materials, &[0, 1], 0x400, 120);
        distribute(&mut storages, &materials, &[0, 1], 0x20, 1800);
        let table = stock_table(&storages, &materials);
        assert_eq!(table[0], 4); // 4 logs of 11 fit in 50
        assert_eq!(table[5], 1800);
    }
}
