//! HappinessComponent (controller interface vtable `0x14015f6a0`, table `FUN_14004b570`):
//! wells, markets, the tavern, chapel, cemetery, doctor and trading post make the homes
//! around them happier; mines and the quarry make them less happy. See
//! docs/CITIZEN-LIFE.md "Happiness buildings".
//!
//! While active, a building is registered with the town manager (`Game +0xf0 → +0x50`) and
//! counts on its kind's map layer within its radius. A citizen's happiness reads the layers
//! at its home's entrance; loitering citizens pick a registered building nearby.

use crate::town::Sim;

impl Sim<'_, '_> {
    /// `FUN_14004aeb0`: the unhappy kind always counts; one that needs workers counts while
    /// its workplace is staffed and switched on; one that needs stock while its storage holds
    /// a matching raw material.
    pub(crate) fn happiness_active(&self, e: usize) -> bool {
        let Some(site) = self.sites.get(&e) else {
            return false;
        };
        let Some(d) = self.site_templates[site.template].happiness else {
            return false;
        };
        if d.kind == 5 {
            return true;
        }
        if d.needs_workers {
            if let Some(w) = self.workforce.workplaces.get(&e) {
                if w.workers.is_empty() || !w.enabled {
                    return false;
                }
            }
        }
        if !d.needs_stock {
            return true;
        }
        self.storages[site.storage]
            .items
            .iter()
            .any(|i| d.mask & self.materials[i.material].flags != 0)
    }

    /// `FUN_14004ac60`: add `delta` to the kind's layer (kinds above 4 count as 5) within the
    /// radius around the map rectangle's centre.
    fn happiness_layer(&mut self, e: usize, delta: i32) {
        let Some(site) = self.sites.get(&e) else {
            return;
        };
        let t = &self.site_templates[site.template];
        let (Some(d), radius) = (t.happiness, t.radius.unwrap_or(0)) else {
            return;
        };
        let layer = if (0..=4).contains(&d.kind) {
            d.kind as usize
        } else {
            5
        };
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        self.cells
            .add_layer(layer, (w >> 1) + x, (h >> 1) + y, radius, delta);
    }

    /// `FUN_14004ae50`.
    fn happiness_register(&mut self, e: usize) {
        self.happiness_layer(e, 1);
        self.happiness.push(e);
        if let Some(s) = self.sites.get_mut(&e) {
            s.happiness_registered = true;
        }
    }

    /// `FUN_14004ae00` (and `FUN_14004b2a0` / `FUN_14004b380` when registered).
    fn happiness_unregister(&mut self, e: usize) {
        if let Some(i) = self.happiness.iter().position(|&x| x == e) {
            self.happiness.remove(i);
        }
        self.happiness_layer(e, -1);
        if let Some(s) = self.sites.get_mut(&e) {
            s.happiness_registered = false;
        }
    }

    fn happiness_registered(&self, e: usize) -> bool {
        self.sites.get(&e).is_some_and(|s| s.happiness_registered)
    }

    /// State 2 and 0xd enter (`FUN_14004b350`).
    pub(crate) fn happiness_enter(&mut self, e: usize) {
        if !self.happiness_registered(e) && self.happiness_active(e) {
            self.happiness_register(e);
        }
    }

    /// State 2 and 0xd scheduled update (`FUN_14004b300`).
    pub(crate) fn happiness_update(&mut self, e: usize) {
        let active = self.happiness_active(e);
        if !self.happiness_registered(e) {
            if active {
                self.happiness_register(e);
            }
        } else if !active {
            self.happiness_unregister(e);
        }
    }

    /// State 3 enter (`FUN_14004b2a0`) and the component's removal (`FUN_14004b380`).
    pub(crate) fn happiness_leave(&mut self, e: usize) {
        if self.happiness_registered(e) {
            self.happiness_unregister(e);
        }
    }

    /// Message 0 (`FUN_14004b0e0`), a citizen's visit: one unit of the first stored raw
    /// material that matches and is not reserved is used up. Not ported: the stock manager's
    /// consumption counter (`+0xc` of the material's slot).
    pub fn happiness_visit(&mut self, e: usize) {
        let Some(site) = self.sites.get(&e) else {
            return;
        };
        let Some(d) = self.site_templates[site.template].happiness else {
            return;
        };
        if !d.needs_stock {
            return;
        }
        let storage = &mut self.storages[site.storage];
        let found = storage
            .items
            .iter()
            .find(|i| d.mask & self.materials[i.material].flags != 0 && 0 < i.count - i.reserved);
        if let Some(m) = found.map(|i| i.material) {
            storage.remove(m, 1);
        }
    }
}
