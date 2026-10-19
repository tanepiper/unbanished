//! Pastures: PastureComponent and its work. The animals themselves (livestock, breeding, products)
//! live in [`crate::nature::wildlife`]. See docs/FARMING.md "Pastures".
//!
//! A built pasture takes waiting livestock of its kind until full, has the young its breeding males
//! sire born, and turns what its animals give (eggs, wool) into piles at the shelter. Its ranchers
//! cull the oldest grown animal while it holds more than the player allows, pick up the products,
//! or walk the pasture, and each visit lets a random animal breed.
//!
//! Original: `FUN_14012d660`, `FUN_1400f8250`.
// vtable 0x14016a360; vtable 0x140167bc0

use crate::behaviour::character::{Action, Component, Message, MoveTo};
use crate::nature::wildlife::{Wild, HUNTED};
use crate::pathfind::{nearest_neighbour, WALKABLE};
use crate::production::herbalist::HerbalistWork;
use crate::scheduler::EntityScheduler;
use crate::terrain::CellType;
use crate::town::Sim;
use crate::work::jobs::{Jobs, Work, WorkKind, NESTED};

impl Sim<'_, '_> {
    /// The livestock's context. Animals born here join the entity manager's buckets with the
    /// iteration's other new entities.
    pub(crate) fn with_wild<R>(&mut self, f: impl FnOnce(&mut Wild) -> R) -> R {
        let mut scheduler = EntityScheduler::default();
        let mut wild = Wild {
            wildlife: &mut *self.wildlife,
            placed: &mut *self.placed,
            templates: self.templates,
            cells: &*self.cells,
            terrain: self.terrain,
            rng: &mut *self.rng,
            scheduler: &mut scheduler,
        };
        f(&mut wild)
    }

    /// State 2 enter (`FUN_14012c590`): take waiting livestock, join the town's pastures, and
    /// the workplace waits for the player to choose a kind.
    pub(crate) fn pasture_enter2(&mut self, e: usize) {
        self.with_wild(|w| w.fill(e));
        self.wildlife.pasture_list.push(e);
        self.set_pasture_workplace(e, false);
    }

    /// State 2 update (`FUN_14012bcb0`): waiting livestock, births, then the products: every
    /// pile's worth gathered becomes a pile on the first free shelter point while the stock
    /// is under its limit (else it is lost).
    pub(crate) fn pasture_update2(&mut self, jobs: &mut Jobs, e: usize) {
        self.with_wild(|w| {
            w.fill(e);
            w.births(e);
        });
        let Some(kind) = self.wildlife.pastures.get(&e).and_then(|p| p.kind) else {
            return;
        };
        // `FUN_1400a4c40`: the pasture's cells remember the kind.
        let n = self.size as i32;
        let area = self.wildlife.pastures[&e].area;
        for y in area[1] as i32..(area[1] + area[3]) as i32 {
            for x in area[0] as i32..(area[0] + area[2]) as i32 {
                self.cells.previous_template[(y * n + x) as usize] = Some(kind);
            }
        }
        let Some(lk) = self.wildlife.livestock_kinds.get(&kind).cloned() else {
            return;
        };
        let Some(product) = lk.product else { return };
        let Some(material) = self.templates[product].pile_material else {
            return;
        };
        let amount = self.materials[material].pile_amount;
        let (count, limit) = self.stock[lk.group.clamp(0, 20) as usize];
        // Deviation: a zero pile size would never leave the loop in the original.
        while 0 < amount && amount < self.wildlife.pastures[&e].produced {
            self.wildlife.pastures.get_mut(&e).unwrap().produced -= amount;
            if limit <= count {
                continue;
            }
            for cell in self.pasture_points(e) {
                let i = cell as usize;
                if let Some(o) = self.cells.objects[i] {
                    if self.piles.contains_key(&o) {
                        continue;
                    }
                    // Anything else there goes (the cell becomes plain ground).
                    self.cells.set_type(i, CellType(2));
                    self.cells.objects[i] = None;
                }
                let yaw = self.rng.rand_tol(2) as f32 * 90.0;
                let pile = self.spawn_pile(product, (cell % n, cell / n), amount, yaw, None);
                // The pasture's tracker and the stock manager's count (`FUN_140059930`,
                // `FUN_14008e460`).
                self.count_made(Some(e), true, material, amount);
                let auto = self
                    .sites
                    .get(&e)
                    .and_then(|s| s.auto_pickup.as_ref())
                    .is_some();
                if auto {
                    if let Some(p) = self.piles.get_mut(&pile) {
                        p.auto_pickup = Some(e);
                    }
                }
                // `FUN_1400c9d70`: registered (its Pickup work) and handed to the AutoPickup.
                self.activate_piles(jobs);
                break;
            }
        }
    }

    /// State 3 enter (`FUN_14012c310`) and the removal slot (`FUN_14012ccb0`): the animals go
    /// back to wait for another pasture.
    pub(crate) fn pasture_leave(&mut self, e: usize) {
        self.with_wild(|w| w.release(e));
    }

    /// Slot `+0x60` with event 4 (`FUN_14012ca10`, sent when building ends, `FUN_14010cad0`):
    /// the shelter's points become plain ground for the products.
    pub(crate) fn pasture_built(&mut self, e: usize) {
        for cell in self.pasture_points(e) {
            let i = cell as usize;
            self.cells.set_type(i, CellType(2));
            self.cells.objects[i] = None;
        }
    }

    /// The InteractComponent's create points (kind 2, `FUN_140128540` / `FUN_1401282f0`): its
    /// own, then its children's (the shelter's), as cells.
    pub(crate) fn pasture_points(&self, e: usize) -> Vec<i32> {
        let mut cells = Vec::new();
        let parts = self
            .sites
            .get(&e)
            .map(|s| s.parts.clone())
            .unwrap_or_default();
        for x in std::iter::once(e).chain(parts) {
            let count = self.templates[self.placed[x].template].create_points.len();
            cells.extend((0..count).map(|i| self.create_cell(x, i)));
        }
        cells
    }

    /// `FUN_1401350d0` on the pasture's workplace.
    fn set_pasture_workplace(&mut self, e: usize, on: bool) {
        self.enable_workplace(e, on);
    }

    /// The pasture's message 0: a rancher's visit.
    ///
    /// Original: `FUN_14012cbf0`.
    pub fn pasture_message(&mut self, e: usize, message: i32) {
        if message == 0 {
            self.with_wild(|w| w.mark_breeder(e));
        }
    }

    // +0x58
    /// PastureUI slot: the kind of animal; choosing the first one switches the work on. The most it
    /// keeps becomes its room.
    ///
    /// Original: `FUN_1400ba110`.
    pub fn set_pasture_kind(&mut self, jobs: &mut Jobs, e: usize, kind: Option<usize>) {
        let Some(old) = self.wildlife.pastures.get(&e).map(|p| p.kind) else {
            return;
        };
        if old.is_none() && kind.is_some() {
            self.wildlife.pastures.get_mut(&e).unwrap().kind = kind;
            self.pasture_action(jobs, e, 2);
        }
        self.wildlife.pastures.get_mut(&e).unwrap().kind = kind;
        let cap = self.wildlife.capacity(e);
        self.wildlife.pastures.get_mut(&e).unwrap().limit = cap;
    }

    // +0x48
    /// PastureUI slot: the slider sets the most animals kept.
    ///
    /// Original: `FUN_1400ba2f0`.
    pub fn set_pasture_limit(&mut self, e: usize, limit: i32) {
        if let Some(p) = self.wildlife.pastures.get_mut(&e) {
            p.limit = limit;
        }
    }

    // +0x68
    /// Whether the window's button `query` is available. 0 (send every animal away) needs room for
    /// all of them in other pastures of the kind, 1 (send half) ten animals and room for five; 2 is
    /// the work toggle's state (bit 2 on).
    ///
    /// Original: `FUN_14012d2a0`.
    pub fn pasture_query(&self, e: usize, query: i32) -> u8 {
        let Some(p) = self.wildlife.pastures.get(&e) else {
            return 0;
        };
        let mut free = 0;
        for &o in &self.wildlife.pasture_list {
            if o != e
                && self
                    .wildlife
                    .pastures
                    .get(&o)
                    .is_some_and(|x| x.kind == p.kind)
            {
                free +=
                    self.wildlife.capacity(o) - self.wildlife.pastures[&o].livestock.len() as i32;
            }
        }
        let has = u8::from(p.kind.is_some());
        let count = p.livestock.len() as i32;
        match query {
            0 if 0 < count && count <= free => has,
            1 if 9 < count && 4 < free => has,
            2 => {
                let on = self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled);
                if on {
                    2 | has
                } else {
                    has
                }
            }
            _ => 0,
        }
    }

    // +0x70
    /// 2 switches the work on or off; 0 sends every animal (1 half of them, rounded down) to other
    /// pastures of the kind, the roomiest first.
    ///
    /// Original: `FUN_14012cf30`.
    pub fn pasture_action(&mut self, jobs: &mut Jobs, e: usize, action: i32) {
        if self.pasture_query(e, action) & 1 == 0 {
            return;
        }
        if action == 2 {
            let on = self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled);
            self.set_pasture_workplace(e, !on);
            if on {
                self.remove_work(jobs, e);
                self.set_pasture_workplace(e, false);
            } else {
                let work = self.pasture_work(e);
                self.set_work(jobs, e, work, 1, true);
                self.set_pasture_workplace(e, true);
            }
            return;
        }
        let count = self.wildlife.pastures[&e].livestock.len() as i32;
        let keep = if action == 0 { 0 } else { count - (count >> 1) };
        let kind = self.wildlife.pastures[&e].kind;
        let room = |w: &crate::nature::wildlife::Wildlife, o: usize| {
            w.capacity(o) - w.pastures[&o].livestock.len() as i32
        };
        let others: Vec<usize> = self
            .wildlife
            .pasture_list
            .iter()
            .copied()
            .filter(|&o| {
                o != e
                    && self
                        .wildlife
                        .pastures
                        .get(&o)
                        .is_some_and(|x| x.kind == kind)
            })
            .collect();
        let mut best = None;
        let mut most = 0;
        for &o in &others {
            let r = room(self.wildlife, o);
            if most < r {
                most = r;
                best = Some(o);
            }
        }
        let send = |sim: &mut Self, o: usize| {
            while keep < sim.wildlife.pastures[&e].livestock.len() as i32
                && 0 < room(sim.wildlife, o)
            {
                let a = sim.wildlife.pastures[&e].livestock[0];
                sim.with_wild(|w| w.assign(a, Some(o)));
            }
        };
        if let Some(b) = best {
            send(self, b);
        }
        for o in others {
            if Some(o) != best {
                send(self, o);
            }
        }
    }

    /// The work (`FUN_1400f8250`), made when the player switches it on.
    pub(crate) fn pasture_work(&self, e: usize) -> Work {
        let profession = self
            .workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer);
        Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::Pasture(HerbalistWork::default()),
        )
    }

    /// Slot `+0x20` (`FUN_1400f7840`) after the AutoPickup offers. With room in the stock: over
    /// the limit, cull the oldest grown animal; else offer the products' pickups at the
    /// shelter (`pasture_stage` marks the rest of the try).
    pub(crate) fn pasture_try(&mut self, work: &mut Work, c: usize) -> u8 {
        if std::mem::take(&mut self.pasture_stage) {
            return self.pasture_walk(work, c);
        }
        let e = work.entity;
        let WorkKind::Pasture(h) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < h.workers.len() as i32 {
            return 1;
        }
        let group = self
            .sites
            .get(&e)
            .and_then(|s| self.site_templates[s.template].pasture)
            .unwrap_or(5);
        let (count, limit) = self.stock[group.clamp(0, 20) as usize];
        self.set_icon(e, crate::work::professions::icon::LIMIT, limit <= count);
        if limit <= count {
            return 1;
        }
        let Some(p) = self.wildlife.pastures.get(&e) else {
            return 0;
        };
        let here = self.citizen_cell(c);
        if p.limit < p.livestock.len() as i32 {
            let Some(a) = self.cull_choice(e) else {
                return 1;
            };
            let animal = &self.wildlife.animals[&a];
            if animal.flags & HUNTED != 0 {
                return 1;
            }
            let at = self.cell_of(animal.character.position);
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, at, here, WALKABLE)
            else {
                return 1;
            };
            if !self.reachable(nb, here) {
                return 1;
            }
            let point = self.ground_at(nb);
            self.cull_sequence(c, work, a, point, at);
            self.took_job(c, work);
            return 2;
        }
        let n = self.size as i32;
        let offers: Vec<i32> = self
            .pasture_points(e)
            .into_iter()
            .filter(|&cell| 0 <= cell && cell < n * n)
            .filter_map(|cell| self.cells.objects[cell as usize])
            .filter_map(|o| self.piles.get(&o).map(|p| p.work))
            .collect();
        if !offers.is_empty() {
            self.offers = offers;
            self.offers_again = true;
            self.pasture_stage = true;
            return NESTED;
        }
        self.pasture_walk(work, c)
    }

    /// The try's end: walk to a random cell of the pasture and tend there (`FUN_1400c0570`
    /// with the pasture's message 0); the rancher joins the work's set.
    fn pasture_walk(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let Some(area) = self.wildlife.pastures.get(&e).map(|p| p.area) else {
            return 0;
        };
        let n = self.size as i32;
        let ry = self.rng.rand_below(area[3] as u32) as i32;
        let rx = self.rng.rand_below(area[2] as u32) as i32;
        let cell = (area[1] as i32 + ry) * n + rx + area[0] as i32;
        let here = self.citizen_cell(c);
        let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, here, WALKABLE) else {
            return 0;
        };
        if !self.reachable(here, nb) {
            return 0;
        }
        let at = self.ground_at(cell);
        let stand = self.ground_at(nb);
        let message = Message {
            component: Component::Pasture,
            message: 0,
            entity: Some(e),
            a: 0,
            b: 0,
        };
        self.tend_sequence(c, work, stand, at, message);
        self.took_job(c, work);
        if let WorkKind::Pasture(h) = &mut work.kind {
            h.workers.insert(c);
        }
        2
    }

    /// `FUN_14012d820`: the oldest animal past its grown age that no one has taken.
    fn cull_choice(&self, e: usize) -> Option<usize> {
        let mut best = None;
        let mut oldest = 0.0f32;
        for &a in &self.wildlife.pastures.get(&e)?.livestock {
            let animal = &self.wildlife.animals[&a];
            if oldest < animal.age && animal.flags & HUNTED == 0 {
                let adult = self.wildlife.animal_kinds[&animal.template].desc.adult;
                if adult < animal.age {
                    oldest = animal.age;
                    best = Some(a);
                }
            }
        }
        best
    }

    /// `FUN_1400c1370` with clearing: the rancher walks next to the animal, works 4 s, removes
    /// it and leaves its raw materials where it stood; the animal is taken (`FUN_14010c730`).
    fn cull_sequence(&mut self, c: usize, work: &mut Work, a: usize, point: [f32; 3], at: i32) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: a as i32,
            },
        );
        let template = self.wildlife.animals[&a].template;
        let materials = self
            .wildlife
            .animal_kinds
            .get(&template)
            .map(|k| k.materials.clone())
            .unwrap_or_default();
        let batch = self.states[c].carried.capacity;
        let workplace = self.states[c].workplace;
        for m in materials {
            self.push(
                c,
                Action::CreateRawMaterial {
                    template: m,
                    cell: at,
                    batch,
                    workplace,
                    factor: 1.0,
                },
            );
        }
        self.push(c, Action::RemoveAnimal { entity: a });
        let perform = self.perform(c, -1000.0, 0xb, 4.0, true);
        self.push(c, perform);
        let walk = MoveTo::new(point, 1, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.with_wild(|w| w.hunted(a));
    }

    /// Slot `+0x38` (`FUN_1400f7790`): the rancher leaves the set, and a culled animal is free
    /// again.
    pub(crate) fn pasture_fail(&mut self, work: &mut Work, c: usize, param: i32) {
        if let WorkKind::Pasture(h) = &mut work.kind {
            h.workers.remove(&c);
        }
        if let Some(animal) = usize::try_from(param)
            .ok()
            .and_then(|a| self.wildlife.animals.get_mut(&a))
        {
            animal.flags &= !HUNTED;
        }
    }
}
