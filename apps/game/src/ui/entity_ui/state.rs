//! The page and control states of an entity window, and its trackers and cemetery counts.

use super::*;

/// EntityUI refresh `FUN_1400676e0`: the host page for the entity's state. Page 2 while it
/// is cleared or built, 3 in states 3..=7 (damaged or being destroyed), else the
/// UIDescription's page (`+0x40`), keeping page 4 once the player opened it.
pub(super) fn entity_page(town: &StartingTown, entity: Entity, finished: i32, active: i32) -> i32 {
    use unbanished_game::work::construction::Controller as C;
    let Entity::Placed(p) = entity else {
        return finished;
    };
    let c = &town.construction;
    let Some(site) = c.sites.get(&p) else {
        return finished;
    };
    let controllers = &c.templates[site.template].controllers;
    let m = &site.machine;
    let state = if m.pending < 0 { m.state } else { m.pending } as i32;
    if !controllers.contains(&C::Build) {
        return if !controllers.contains(&C::ClearArea) || state != 0 {
            finished
        } else {
            2
        };
    }
    if (3..8).contains(&state) {
        return 3;
    }
    let t = &c.templates[site.template];
    // `FUN_14010e860`: BuildDescription `+0x30` work × stages <= progress.
    if t.work * site.stages > site.progress {
        return 2;
    }
    if state != 0 {
        if active == 4 {
            4
        } else {
            finished
        }
    } else {
        2
    }
}

/// Slot `+0x68` of the `kind` component of `e` for control `index`: bit 0 enabled, bit 1 on.
pub(super) fn control_state(town: &StartingTown, e: usize, kind: u32, index: i32) -> u8 {
    use unbanished_game::work::control::kind as k;
    let Some(site) = town.construction.sites.get(&e) else {
        return 0;
    };
    let state = if site.machine.pending < 0 {
        site.machine.state
    } else {
        site.machine.pending
    };
    let enabled = town
        .construction
        .workforce
        .workplaces
        .get(&e)
        .is_some_and(|w| w.enabled);
    let on = |b: bool| if b { 2 } else { 0 };
    match kind {
        // `FUN_140120270`: plant now in 2, harvest now in 10, farming; all need a crop.
        k::CROP_FIELD => {
            let Some(f) = &site.crop_field else { return 0 };
            let crop = f.crop.is_some();
            match index {
                0 => u8::from(state == 2 && crop),
                1 => u8::from(state == 0xa && crop),
                2 => on(f.enabled) | u8::from(crop),
                _ => 0,
            }
        }
        // `FUN_1400525f0`: plant (no work and room, or no trees), harvest (in 10, not this
        // year), tending, cut (standing trees, not in 0x14); all need a tree.
        k::ORCHARD => {
            let Some(o) = &site.orchard else { return 0 };
            let tree = o.tree.is_some();
            let standing = o.trees.iter().filter(|t| t.state == 1).count();
            let has_work =
                site.work_id >= 0 && town.construction.jobs.works.contains_key(&site.work_id);
            match index {
                0 => u8::from(
                    ((!has_work && standing < o.trees.len()) || o.trees.is_empty()) && tree,
                ),
                1 => u8::from(state == 0xa && o.year != town.construction.year && tree),
                2 => on(o.enabled) | u8::from(tree),
                3 => u8::from(0 < standing && state != 0x14),
                _ => 0,
            }
        }
        // `FUN_14012d2a0`: room in the other pastures of the kind; the work toggle.
        k::PASTURE => {
            let w = &town.wildlife;
            let Some(p) = w.pastures.get(&e) else {
                return 0;
            };
            let mut free = 0;
            for &o in &w.pasture_list {
                if o != e && w.pastures.get(&o).is_some_and(|x| x.kind == p.kind) {
                    free += w.capacity(o) - w.pastures[&o].livestock.len() as i32;
                }
            }
            let has = u8::from(p.kind.is_some());
            let count = p.livestock.len() as i32;
            match index {
                0 if 0 < count && count <= free => has,
                1 if 9 < count && 4 < free => has,
                2 => on(enabled) | has,
                _ => 0,
            }
        }
        // `FUN_14005caa0`, `FUN_140044870`: the workplace's switch.
        k::TRADE | k::ATTENDANCE => on(enabled) | 1,
        // `FUN_140124df0`: planting and cutting.
        k::FORESTER => match (index, &site.forester) {
            (1, Some(f)) => on(f.plant) | 1,
            (2, Some(f)) => on(f.cut) | 1,
            _ => 1,
        },
        // `FUN_1401309f0`: on unless idle (0xe); available in 0xd and 0xe unless switched off.
        _ => {
            let disabled = site.produce.as_ref().is_some_and(|p| p.disabled);
            on(state != 0xe) | u8::from(!disabled && (state == 0xd || state == 0xe))
        }
    }
}

/// ProductionUI's view of the TrackerComponent (`FUN_14007af20`): in a new year this year's
/// list is last year's.
pub(super) fn tracker_lists(town: &StartingTown, p: usize) -> Option<[Vec<(usize, i32)>; 2]> {
    let t = town.construction.trackers.get(&p)?;
    if t.year != town.construction.year {
        return Some([Vec::new(), t.this_year.clone()]);
    }
    Some([t.this_year.clone(), t.last_year.clone()])
}

/// CemeteryUI's counts: graves whose cell holds a tombstone (`FUN_140047730`), and graves.
pub(super) fn cemetery_counts(town: &StartingTown, p: usize) -> Option<(i32, i32)> {
    if !town.construction.cemeteries.contains(&p) {
        return None;
    }
    let graves = &town.construction.sites.get(&p)?.graves;
    let used = graves
        .iter()
        .filter(|&&c| {
            town.cells.objects[c as usize]
                .is_some_and(|o| town.construction.tombstones.contains_key(&o))
        })
        .count();
    Some((used as i32, graves.len() as i32))
}
