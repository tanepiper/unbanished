//! The town statistics a goal counts: buildings and people by kind, the statistic series.

use super::*;

/// The known crops (0), trees (1) or livestock (2) by their SelectDescription (`+0x40`).
pub(super) fn known_of(archive: Option<&Archive>, town: &StartingTown, kind: i32) -> i32 {
    let Some(archive) = archive else { return 0 };
    town.construction
        .known
        .iter()
        .filter(|&&t| {
            let f = file(&town.templates[t].reference).replace('\\', "/");
            archive
                .read_ref(&format!("{f}:select"))
                .ok()
                .and_then(|d| select_kind(&d))
                == Some(kind)
        })
        .count() as i32
}

/// SelectDescription `+0x40`.
pub(super) fn select_kind(data: &[u8]) -> Option<i32> {
    use unbanished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| {
            r.external_ref()?;
            r.string()?;
            r.external_ref()?;
            r.string()?;
            let k = r.i32()?;
            r.skip_rest();
            Ok(k)
        })
    })
    .ok()
    .map(|(_, k)| k)
}

/// `FUN_14009f0` … `FUN_1400df9b0`: a statistic. `flags` is StatGoal `+0x78`, `index` `+0x7c`.
pub(super) fn stat(
    x: &GoalContext,
    archive: Option<&Archive>,
    kind: i32,
    r: &str,
    flags: u32,
    index: i32,
) -> i32 {
    let town = x.town;
    let rules = &town.rules;
    let education = town.agents.rules().education;
    let live = || {
        town.citizens
            .iter()
            .zip(&town.agents.states)
            .enumerate()
            .filter(|(_, (c, _))| c.alive)
    };
    let working =
        |c: &unbanished_game::people::citizens::Citizen| c.is_adult(rules) && c.school.is_none();
    let material = || town.materials.iter().position(|m| same(&m.reference, r));
    match kind {
        // `FUN_1400df8e0`: educated working adults.
        0 => live()
            .filter(|(_, (c, _))| working(c) && education <= c.education)
            .count() as i32,
        // `FUN_1400df770`: working adults with the material's tool (or one past use).
        1 => {
            let uses = material().map_or(i32::MIN, |m| town.materials[m].tool_uses);
            live()
                .filter(|(_, (c, s))| working(c) && (s.tool_status == uses || s.tool_status < 0))
                .count() as i32
        }
        // `FUN_1400df620`: citizens in a coat of the material's kind (warm, flag `0x8000`).
        2 => {
            let warm = material().is_some_and(|m| town.materials[m].flags & 0x8000 != 0);
            live()
                .filter(|(_, (_, s))| {
                    let v = s.clothing_status;
                    ((v == 2 || v == 4) && warm) || ((v == 1 || v == 4) && !warm)
                })
                .count() as i32
        }
        // `FUN_1400df570`, `FUN_1400df4c0`: the mean happiness and health.
        3 | 4 => {
            let v: Vec<f32> = live()
                .map(|(_, (_, s))| if kind == 3 { s.happiness } else { s.health })
                .collect();
            if v.is_empty() {
                0
            } else {
                (v.iter().sum::<f32>() / v.len() as f32) as i32
            }
        }
        // Stock manager `+0xa8`: the population.
        5 => live().count() as i32,
        6 => live().filter(|(_, (c, _))| working(c)).count() as i32,
        7 => live().filter(|(_, (c, _))| c.school.is_some()).count() as i32,
        8 => live()
            .filter(|(_, (c, _))| !c.is_adult(rules) && c.school.is_none())
            .count() as i32,
        // `FUN_1400df1a0`: working adults of the profession.
        9 => {
            let p = town
                .construction
                .professions
                .list
                .iter()
                .position(|p| p.reference.eq_ignore_ascii_case(r));
            live()
                .filter(|(_, (c, s))| working(c) && Some(s.profession) == p)
                .count() as i32
        }
        // `FUN_1400df150`, `FUN_1400df0e0`: the game's terrain or climate.
        10 => same(x.terrain, r) as i32,
        11 => same(x.climate, r) as i32,
        // `FUN_1400df050`: graves with a tombstone.
        12 => town
            .construction
            .cemeteries
            .iter()
            .filter_map(|&p| town.construction.sites.get(&p))
            .flat_map(|s| s.graves.iter())
            .filter(|&&c| {
                town.cells.objects[c as usize]
                    .is_some_and(|o| town.construction.tombstones.contains_key(&o))
            })
            .count() as i32,
        // Stock manager `+0x1ac`: the value traded away.
        13 => town.construction.trade_value,
        // `FUN_1400deb20`: what the storages, homes and trading posts (of the template) hold
        // of the flags.
        14 => {
            let mut storages: Vec<(usize, usize)> = town
                .storage_sites
                .iter()
                .map(|s| (s.entity, s.storage))
                .collect();
            for (&e, h) in &town.construction.homes {
                storages.push((e, h.storage));
            }
            for &p in &town.construction.trading_posts {
                if let Some(s) = town.construction.sites.get(&p) {
                    storages.push((p, s.storage));
                }
            }
            storages.sort_unstable();
            storages.dedup();
            storages
                .iter()
                .filter(|(e, _)| {
                    r.is_empty() || same(&town.templates[town.placed[*e].template].reference, r)
                })
                .filter_map(|(_, s)| town.storages.get(*s))
                .flat_map(|s| s.items.iter())
                .filter(|i| town.materials[i.material].flags & flags != 0)
                .map(|i| i.count)
                .sum()
        }
        // `FUN_1400dea30`, `FUN_1400de940`, `FUN_1400de850`: known livestock, crops, trees.
        15 => known_of(archive, town, 2),
        16 => known_of(archive, town, 0),
        17 => known_of(archive, town, 1),
        // `FUN_1400de7a0`: the animals in pastures of the kind.
        18 => town
            .wildlife
            .pastures
            .values()
            .filter(|p| {
                p.kind
                    .is_some_and(|k| same(&town.templates[k].reference, r))
            })
            .map(|p| p.livestock.len() as i32)
            .sum(),
        // `FUN_1400de240`: the kinds of the flags this year's workplaces (of the template)
        // made.
        19 => {
            let mut kinds = std::collections::BTreeSet::new();
            for e in workplaces(town, r) {
                if let Some(t) = town.construction.trackers.get(&e) {
                    for &(m, _) in &t.this_year {
                        if town.materials[m].flags & flags != 0 {
                            kinds.insert(m);
                        }
                    }
                }
            }
            kinds.len() as i32
        }
        // `FUN_1400de120`: the stock slot's production over the last `index` years.
        20 => {
            let slot = [
                (0x20000, 8),
                (0x4000, 7),
                (0x40, 3),
                (0x20, 5),
                (0x2000, 6),
                (0x1000, 2),
                (0x800, 1),
                (0x400, 0),
                (0x200, 4),
            ]
            .iter()
            .find(|(f, _)| flags & f != 0)
            .map_or(4, |&(_, s)| s);
            let series = 8 + 3 * slot + 2;
            let into = ((x.year_seconds / 3600.0) * 48.0) as i32;
            let into = if 0x2f < into { 0 } else { into };
            let year = x.year as i32;
            sum(
                x.statistics,
                series,
                (year - index - 1) * 48,
                year * 48 - 48 + into,
            )
        }
        // `FUN_1400dddb0`: what this year's workplaces of the template made of the flags.
        21 => workplaces(town, r)
            .iter()
            .filter_map(|e| town.construction.trackers.get(e))
            .flat_map(|t| t.this_year.iter())
            .filter(|(m, _)| town.materials[*m].flags & flags != 0)
            .map(|(_, n)| n)
            .sum(),
        // `FUN_1400ddbb0`: the workers at the template's workplaces.
        22 => workplaces(town, r)
            .iter()
            .filter_map(|e| town.construction.workforce.workplaces.get(e))
            .map(|w| w.workers.len() as i32)
            .sum(),
        // `FUN_1400ddb70`: the map's built road cells of a kind (`Game +0x110 → +0x220`).
        23 => (0..town.cells.road_kind.len())
            .filter(|&i| {
                town.cells.road_kind[i] as i32 == index && town.cells.road_built[i] == 0xff
            })
            .count() as i32,
        // `FUN_1400ddac0`: the residences (of the template).
        24 => town
            .construction
            .workforce
            .residences
            .iter()
            .filter(|&&e| {
                r.is_empty() || same(&town.templates[town.placed[e].template].reference, r)
            })
            .count() as i32,
        // `FUN_1400dd680`: the template's buildings standing.
        25 => entities(town, r).filter(|&e| standing(town, e)).count() as i32,
        // `Game +0x128`: the year.
        26 => x.year as i32,
        // `Game +0x34`: the map size.
        27 => x.size,
        // Stock manager `+0x1b0`: nomads taken in.
        28 => town.construction.nomads_admitted,
        // `FUN_1400dd1c0`: the longest side of the template's standing buildings.
        29 => entities(town, r)
            .filter(|&e| standing(town, e))
            .map(|e| {
                let f = town.placed[e].footprint;
                f.0.max(f.1)
            })
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

/// `FUN_1400963f0`: a series' sum over samples `from..=to`, clamped to what exists.
pub(super) fn sum(s: &Statistics, series: usize, from: i32, to: i32) -> i32 {
    let Some(v) = s.series.get(series) else {
        return 0;
    };
    let from = from.max(0);
    let to = to.min(v.len() as i32 - 1);
    if to < from {
        return 0;
    }
    v[from as usize..=to as usize].iter().sum()
}
