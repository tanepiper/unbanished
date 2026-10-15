//! StorageComponent ledgers and town storage search (`Game +0xf0`).
//! See docs/CITIZEN-AI.md and docs/RESOURCE-PRODUCTION-FLOW.md.

use crate::random::Random;

/// RawMaterialDescription: a template shared by ledger entries, not a stored entity.
#[derive(Debug, Clone, Default)]
pub struct RawMaterial {
    pub reference: String,
    /// `+0x48`: weighted storage units per item.
    pub size: i32,
    /// `+0x5c`: category flags (distinct from the search's category index).
    pub flags: u32,
    /// `+0x54`, sent to Citizen message 1 by FUN_1400c2540.
    pub tool_uses: i32,
    /// `+0x58`: FUN_140094410 prefers the last eligible entry with this set.
    pub preferred: bool,
    /// `+0x4c`: a crop field leaves a pile on every this-many cells (`FUN_14011f7e0`).
    pub pile_spacing: i32,
}

/// StorageComponent's 16-byte entry at `+0x28`: template, quantity, reserved quantity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub material: usize,
    pub count: i32,
    pub reserved: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Storage {
    /// Component `+0x38` and `+0x3c`.
    pub capacity: i32,
    pub accepts: u32,
    pub items: Vec<Item>,
}

impl Storage {
    /// Port of FUN_1401323e0: weighted occupancy (includes reserved goods).
    pub fn used(&self, materials: &[RawMaterial]) -> i32 {
        self.items.iter().fold(0i32, |n, e| {
            n.wrapping_add(materials[e.material].size.wrapping_mul(e.count))
        })
    }

    /// Port of FUN_140132250 / FUN_140131a30.
    pub fn fits(&self, materials: &[RawMaterial], material: usize, count: i32) -> bool {
        self.accepts & materials[material].flags != 0
            && materials[material]
                .size
                .wrapping_mul(count)
                .wrapping_add(self.used(materials))
                <= self.capacity
    }

    pub fn entry(&self, material: usize) -> Option<&Item> {
        self.items.iter().find(|e| e.material == material)
    }

    /// Port of FUN_14011bc90: quantity available for another task.
    pub fn available(&self, material: usize) -> i32 {
        self.entry(material)
            .map_or(0, |e| e.count.wrapping_sub(e.reserved))
    }

    /// Port of FUN_1401324d0: merge in place, otherwise append with no reservation.
    pub fn add(&mut self, material: usize, count: i32) {
        if let Some(e) = self.items.iter_mut().find(|e| e.material == material) {
            e.count = e.count.wrapping_add(count);
        } else {
            self.items.push(Item {
                material,
                count,
                reserved: 0,
            });
        }
    }

    /// Port of FUN_140132620: subtract, erase exactly-zero entries preserving order; no clamp.
    pub fn remove(&mut self, material: usize, count: i32) {
        if let Some(i) = self.items.iter().position(|e| e.material == material) {
            self.items[i].count = self.items[i].count.wrapping_sub(count);
            if self.items[i].count == 0 {
                self.items.remove(i);
            }
        }
    }

    /// FUN_1400cb980 / FUN_1400cf700: reserve when building a task, not on arrival.
    pub fn reserve(&mut self, material: usize, count: i32) {
        if let Some(e) = self.items.iter_mut().find(|e| e.material == material) {
            e.reserved = e.reserved.wrapping_add(count);
        }
    }

    /// FUN_1400cb300 / FUN_1400cf140 (abort), also used before consuming/transferring.
    pub fn unreserve(&mut self, material: usize, count: i32) {
        if let Some(e) = self.items.iter_mut().find(|e| e.material == material) {
            e.reserved = e.reserved.wrapping_sub(count).max(0);
        }
    }

    /// Port of FUN_140094410: search category 0 food, 1 fuel, 2 tool, 3 herb, 4 clothing.
    /// Even a single eligible unpreferred material draws Rand(1).
    pub fn find_item(
        &self,
        materials: &[RawMaterial],
        category: u32,
        rng: &mut Random,
    ) -> Option<usize> {
        let bit = *[5, 6, 9, 13, 14].get(category as usize)?;
        let eligible = |e: &&Item| {
            materials[e.material].flags & (1 << bit) != 0 && e.count.wrapping_sub(e.reserved) > 0
        };
        let count = self.items.iter().filter(eligible).count();
        if count == 0 {
            return None;
        }
        let preferred = self
            .items
            .iter()
            .filter(eligible)
            .enumerate()
            .filter(|(_, e)| materials[e.material].preferred)
            .map(|(i, _)| i)
            .last();
        let index = preferred.unwrap_or_else(|| rng.rand_below(count as u32) as usize);
        self.items
            .iter()
            .filter(eligible)
            .nth(index)
            .map(|e| e.material)
    }
}

/// Registration in the manager's ordinary (`+0/+8`) or market (`+0x10/+0x18`) list.
#[derive(Debug, Clone)]
pub struct StorageSite {
    pub entity: usize,
    pub storage: usize,
    /// StorageLocationDescription `+0x20`; zero uses StorageComponent `+0x3c`.
    pub location_flags: u32,
    /// RadiusDescription `+0x28`; Some marks a market-list entry.
    pub radius: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StorageMatch {
    pub site: usize,
    pub storage: usize,
    pub material: usize,
    pub target: [f32; 3],
}

/// Geometry dependencies of FUN_140093200, independently implemented/emulated.
pub trait StorageSearchMap {
    fn entrance(&mut self, entity: usize, rng: &mut Random) -> i32;
    fn rank(&mut self, a: i32, b: i32) -> i32;
    fn market_position(
        &mut self,
        entity: usize,
        from: i32,
        rng: &mut Random,
    ) -> Option<(i32, [f32; 3])>;
    fn storage_position(&mut self, entity: usize, from: i32) -> Option<(i32, [f32; 3])>;
}

/// Port of FUN_140093200: strict rank improvements, radius-list precedence, then storage.
/// The covered-home flag is sticky after eligible material selection, even if its slot fails.
#[allow(clippy::too_many_arguments)]
pub fn find_storage(
    sites: &[StorageSite],
    storages: &[Storage],
    materials: &[RawMaterial],
    category: u32,
    cell: i32,
    home: i32,
    map: &mut impl StorageSearchMap,
    rng: &mut Random,
) -> Option<StorageMatch> {
    let mut best = i32::MAX;
    let mut found = None;
    let mut covered = false;
    for (site, s) in sites.iter().enumerate().filter(|(_, s)| s.radius.is_some()) {
        let entrance = map.entrance(s.entity, rng);
        let in_radius = home >= 0 && map.rank(entrance, home) <= s.radius.unwrap();
        if map.rank(entrance, cell) < best {
            if let Some(material) = storages[s.storage].find_item(materials, category, rng) {
                covered |= in_radius;
                if let Some((rank, target)) = map.market_position(s.entity, cell, rng) {
                    if rank >= 0 && rank < best {
                        best = rank;
                        found = Some(StorageMatch {
                            site,
                            storage: s.storage,
                            material,
                            target,
                        });
                    }
                }
            }
        }
    }
    if covered && found.is_some() {
        return found;
    }
    for (site, s) in sites.iter().enumerate().filter(|(_, s)| s.radius.is_none()) {
        let entrance = map.entrance(s.entity, rng);
        if map.rank(entrance, cell) < best {
            let inventory = &storages[s.storage];
            if let Some(material) = inventory.find_item(materials, category, rng) {
                let flags = if s.location_flags == 0 {
                    inventory.accepts
                } else {
                    s.location_flags
                };
                if materials[material].flags & flags != 0 {
                    if let Some((rank, target)) = map.storage_position(s.entity, cell) {
                        if rank >= 0 && rank < best {
                            best = rank;
                            found = Some(StorageMatch {
                                site,
                                storage: s.storage,
                                material,
                                target,
                            });
                        }
                    }
                }
            }
        }
    }
    found
}

/// Port of FUN_140092a60: the storage nearest `cell` (entrance path rank) that holds
/// `count` of `material` (`put` false) or can take them (`put` true). Ordinary storages, then
/// markets; `exclude` is skipped. A StorageLocation must accept the material's flags. On a
/// strict entrance-rank improvement the target point (slot scan, or a market use point) must
/// exist with its own rank below the best, and the best becomes the entrance rank.
#[allow(clippy::too_many_arguments)]
pub fn find_material_storage(
    sites: &[StorageSite],
    storages: &[Storage],
    materials: &[RawMaterial],
    material: usize,
    count: i32,
    put: bool,
    cell: i32,
    exclude: Option<usize>,
    map: &mut impl StorageSearchMap,
    rng: &mut Random,
) -> Option<StorageMatch> {
    let mut best = i32::MAX;
    let mut found = None;
    for market in [false, true] {
        for (site, s) in sites.iter().enumerate() {
            if s.radius.is_some() != market || Some(s.entity) == exclude {
                continue;
            }
            let inventory = &storages[s.storage];
            if !market {
                let flags = if s.location_flags == 0 {
                    inventory.accepts
                } else {
                    s.location_flags
                };
                if materials[material].flags & flags == 0 {
                    continue;
                }
            }
            let ok = if put {
                inventory.fits(materials, material, count)
            } else {
                inventory.available(material).wrapping_sub(count) >= 0
            };
            if !ok {
                continue;
            }
            let entrance = map.entrance(s.entity, rng);
            let rank = map.rank(entrance, cell);
            if rank < best {
                let point = if market {
                    map.market_position(s.entity, cell, rng)
                } else {
                    map.storage_position(s.entity, cell)
                };
                if let Some((r, target)) = point {
                    if r >= 0 && r < best {
                        best = rank;
                        found = Some(StorageMatch {
                            site,
                            storage: s.storage,
                            material,
                            target,
                        });
                    }
                }
            }
        }
    }
    found
}

/// Port of FUN_1400cb420 (ConsumeRawMaterial, vtable 0x140166050).
/// Reserved requests bypass the availability check; mutations happen in list order.
pub fn consume_stored(storage: &mut Storage, items: &[(usize, i32)], reserved: bool) -> bool {
    // Not ported: manager +0x170 historical consumption counters (current stock uses the ledger).
    if items.is_empty() {
        return false;
    }
    for &(material, count) in items {
        if reserved {
            storage.unreserve(material, count);
        } else if storage.available(material) < count {
            return false;
        }
        storage.remove(material, count);
    }
    true
}

/// Port of FUN_1400cf210 (PickupStoredRawMaterial, vtable 0x1401663a0).
/// FUN_1400cf5a0 serializes citizen/source references, these material/count pairs, and partial.
/// Nonpositive requests use the citizen ledger's remaining weighted capacity.
pub fn pickup_stored(
    source: &mut Storage,
    carried: &mut Storage,
    materials: &[RawMaterial],
    items: &[(usize, i32)],
    partial: bool,
) -> bool {
    for &(material, requested) in items {
        let mut count = if requested > 0 {
            requested
        } else {
            (carried.capacity - carried.used(materials)) / materials[material].size
        };
        let available = source.entry(material).map_or(0, |e| e.count);
        if partial {
            count = count.min(available);
            if count == 0 {
                return false;
            }
        } else if available < count {
            return false;
        }
        source.unreserve(material, count);
        source.remove(material, count);
        carried.add(material, count);
    }
    true
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
        for e in &s.items {
            let (m, n) = (e.material, e.count);
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
            ..Default::default()
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
