//! Finding a storage: one with room, scanning them all, and one holding a material.

use super::*;

/// Strict rank improvements, radius-list precedence, then storage. The covered-home flag is sticky
/// after eligible material selection, even if its slot fails. `owner` is the acting player's town:
/// a separate towns game searches its own storage sites only.
///
/// Original: `FUN_140093200`.
#[allow(clippy::too_many_arguments)]
pub fn find_storage(
    sites: &[StorageSite],
    storages: &[Storage],
    materials: &[RawMaterial],
    category: u32,
    cell: i32,
    home: i32,
    owner: u8,
    map: &mut impl StorageSearchMap,
    rng: &mut Random,
) -> Option<StorageMatch> {
    let (_, found) = scan_storages(
        sites,
        storages,
        materials,
        category,
        cell,
        Some(home),
        owner,
        i32::MAX,
        map,
        rng,
    );
    found
}

/// The markets (radius list), then the storages, each taking a strictly better rank than `best`.
/// With `home`, a market whose radius covers the home ends the search after the markets; without
/// it, every storage is weighed. Returns the best rank and its match. `owner`'s sites only.
///
/// Original: `FUN_140093200`; the scan without a home is `FUN_140093840`.
#[allow(clippy::too_many_arguments)]
pub fn scan_storages(
    sites: &[StorageSite],
    storages: &[Storage],
    materials: &[RawMaterial],
    category: u32,
    cell: i32,
    home: Option<i32>,
    owner: u8,
    best: i32,
    map: &mut impl StorageSearchMap,
    rng: &mut Random,
) -> (i32, Option<StorageMatch>) {
    let mut best = best;
    let mut found = None;
    let mut covered = false;
    for (site, s) in sites
        .iter()
        .enumerate()
        .filter(|(_, s)| s.radius.is_some() && s.owner == owner)
    {
        let entrance = map.entrance(s.entity, rng);
        let in_radius = home.is_some_and(|h| h >= 0 && map.rank(entrance, h) <= s.radius.unwrap());
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
        return (best, found);
    }
    for (site, s) in sites
        .iter()
        .enumerate()
        .filter(|(_, s)| s.radius.is_none() && s.owner == owner)
    {
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
    (best, found)
}

/// The storage nearest `cell` (entrance path rank) that holds `count` of `material` (`put` false)
/// or can take them (`put` true). Ordinary storages, then markets; `exclude` is skipped, and only
/// `owner`'s sites are searched. A StorageLocation must accept the material's flags. On a strict
/// entrance-rank improvement the target point (slot scan, or a market use point) must exist with its
/// own rank below the best, and the best becomes the entrance rank.
///
/// Original: `FUN_140092a60`.
#[allow(clippy::too_many_arguments)]
pub fn find_material_storage(
    sites: &[StorageSite],
    storages: &[Storage],
    materials: &[RawMaterial],
    material: usize,
    count: i32,
    put: bool,
    cell: i32,
    owner: u8,
    exclude: Option<usize>,
    map: &mut impl StorageSearchMap,
    rng: &mut Random,
) -> Option<StorageMatch> {
    let mut best = i32::MAX;
    let mut found = None;
    for market in [false, true] {
        for (site, s) in sites.iter().enumerate() {
            if s.radius.is_some() != market || Some(s.entity) == exclude || s.owner != owner {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat;
    impl StorageSearchMap for Flat {
        fn entrance(&mut self, entity: usize, _rng: &mut Random) -> i32 {
            entity as i32
        }
        fn rank(&mut self, a: i32, b: i32) -> i32 {
            (a - b).abs()
        }
        fn market_position(&mut self, _: usize, _: i32, _: &mut Random) -> Option<(i32, [f32; 3])> {
            None
        }
        fn storage_position(&mut self, _: usize, _: i32) -> Option<(i32, [f32; 3])> {
            Some((0, [0.0; 3]))
        }
    }

    fn site(owner: u8, entity: usize, storage: usize) -> StorageSite {
        StorageSite {
            owner,
            entity,
            storage,
            location_flags: 0,
            radius: None,
        }
    }

    /// A separate towns game's search takes the acting player's sites, not the nearest of all.
    #[test]
    fn a_storage_search_takes_only_the_owners_sites() {
        let materials = vec![RawMaterial {
            flags: 1,
            size: 1,
            ..Default::default()
        }];
        let storages: Vec<Storage> = (0..2)
            .map(|_| Storage {
                capacity: 10,
                accepts: 1,
                items: Vec::new(),
            })
            .collect();
        // The world's storage is nearest; player 1's is farther.
        let sites = vec![site(0, 1, 0), site(1, 2, 1)];
        let mut rng = Random::new(1);
        let found = |owner: u8, rng: &mut Random| {
            find_material_storage(
                &sites, &storages, &materials, 0, 1, true, 0, owner, None, &mut Flat, rng,
            )
            .map(|f| f.storage)
        };
        assert_eq!(found(0, &mut rng), Some(0));
        assert_eq!(found(1, &mut rng), Some(1));
    }
}
