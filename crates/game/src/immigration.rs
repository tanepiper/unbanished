//! Immigration: ImmigrationComponent (the town hall, vtable `0x14015fc50`, states
//! `FUN_14004f020`) and the nomads it calls (NomadComponent, vtable `0x140160140`, states
//! `FUN_1400507d0`). See docs/IMMIGRATION.md.
//!
//! Once the town is big enough and has its market and trading post, the town hall now and then
//! (in spring) calls a band of nomads from a map edge. They walk to it and wait; the player
//! takes them in (they become citizens, in families of four, bringing a disease risk) or they
//! leave, at the latest in the summer.

use banished_formats::game::CharacterDescription;

use crate::character::{Action, Character, MoveTo};
use crate::construction::Callbacks;
use crate::scheduler::ticks;
use crate::town::Sim;

/// `DAT_140178934`.
const PERIOD: f32 = 300.0;
/// Event type 14: nomads arrived.
const ARRIVED: i32 = 0xe;
/// `Map::Reachable` mask of the edge search.
const MASK: u32 = 0xe07;

/// A resolved ImmigrationDescription (serializer `FUN_14004f150`, constructor `FUN_14004f300`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImmigrationTemplate {
    /// `+0x20`: the nomad template.
    pub nomad: usize,
    /// `+0x28`: the population the town needs.
    pub population: i32,
    /// `+0x30`: templates the town must have placed (Market, TradingPost).
    pub required: Vec<usize>,
    /// `+0x40`, `+0x44`: the band's size as a share of the population.
    pub share: (f32, f32),
    /// `+0x48`, `+0x4c`: months until the next band (`+0x48 + RandFloat × +0x4c`).
    pub wait: (f32, f32),
    /// `+0x50`, `+0x54`: the months a band may come in.
    pub months: (i32, i32),
    /// `+0x58`: from this month waiting nomads leave (not serialized; 7).
    pub leave: i32,
    /// `+0x5c`: their map-edge margin (not serialized; 10).
    pub margin: i32,
    /// `+0x60`, `+0x64`: the visitors' disease risk and how many months it lasts.
    pub risk: f32,
    pub risk_months: i32,
}

/// ImmigrationComponent: `+0x48` until a band may come, `+0x50` the nomads arrived, `+0x58`
/// the band.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Immigration {
    pub timer: i64,
    pub arrived: i32,
    pub nomads: Vec<usize>,
}

/// NomadDescription (serializer `FUN_140050880`): `+0x20` the citizen template they become;
/// `+0x28` / `+0x30` the men's and women's material (presentation: `StartingTown::nomad_materials`);
/// and the CharacterDescription.
#[derive(Debug, Clone)]
pub struct NomadKind {
    pub character: CharacterDescription,
}

/// NomadComponent: its state (0 walking to the town hall, 1 waiting, 2 leaving), `+0x28` the
/// town hall, `+0x30` where it came from, `+0x3c` where it waits, `+0x48` until it wanders,
/// `+0x50` its model variant (0 men, 1 women).
pub struct Nomad {
    pub template: usize,
    pub character: Character,
    pub state: i8,
    pub town_hall: Option<usize>,
    pub spawn: [f32; 3],
    pub target: [f32; 3],
    pub timer: i64,
    pub variant: usize,
}

impl Sim<'_, '_> {
    fn immigration_desc(&self, e: usize) -> Option<&ImmigrationTemplate> {
        self.site_templates[self.sites.get(&e)?.template]
            .immigration
            .as_ref()
    }

    fn immigration(&mut self, e: usize) -> Option<&mut Immigration> {
        self.sites.get_mut(&e)?.immigration.as_mut()
    }

    /// The calendar's month (`seconds / 3600 × 12`, past 11 is 0).
    fn calendar_month(&self) -> i32 {
        let m = (self.calendar / 3600.0 * 12.0) as i32;
        if 11 < m {
            0
        } else {
            m
        }
    }

    /// `FUN_14004ed50`: the town has its population and a placed entity of every required
    /// template (the town manager's zones by template, `+0x160`).
    fn immigration_wanted(&self, e: usize) -> bool {
        let Some(d) = self.immigration_desc(e) else {
            return false;
        };
        if self.population < d.population {
            return false;
        }
        d.required.iter().all(|&t| {
            self.sites.iter().any(|(&s, site)| {
                !site.destroyed && self.placed[s].alive && self.placed[s].template == t
            })
        })
    }

    /// State 2 enter and update (`FUN_14004dbf0`, `FUN_14004db80`).
    pub(crate) fn immigration_idle(&mut self, e: usize) {
        if self.immigration_wanted(e) {
            self.machine(e).request(0xf);
        }
    }

    /// State 0xf enter (`FUN_14004db00`): `(+0x48 + RandFloat × +0x4c)` × 300 s.
    pub(crate) fn immigration_wait(&mut self, e: usize) {
        let Some(d) = self.immigration_desc(e).cloned() else {
            return;
        };
        let f = self.rng.rand_float();
        let t = ticks((f * d.wait.1 + d.wait.0) * PERIOD);
        if let Some(i) = self.immigration(e) {
            i.timer = t;
        }
    }

    /// State 0xf update (`FUN_14004da30`): when the wait is over, in the season, a band comes
    /// (0x10) or the town no longer qualifies (2).
    pub(crate) fn immigration_waiting(&mut self, e: usize, dt: i64) {
        let Some(d) = self.immigration_desc(e).cloned() else {
            return;
        };
        let Some(i) = self.immigration(e) else { return };
        i.timer -= dt;
        let due = i.timer < 1;
        let month = self.calendar_month();
        if due && d.months.0 <= month && month <= d.months.1 {
            let next = if self.immigration_wanted(e) { 0x10 } else { 2 };
            self.machine(e).request(next);
        }
    }

    /// State 0x10 enter (`FUN_14004d9f0`) and update (`FUN_14004d9d0`): a new band (again
    /// while none could be placed).
    pub(crate) fn immigration_call(&mut self, e: usize, enter: bool) {
        if enter {
            if let Some(i) = self.immigration(e) {
                i.arrived = 0;
                i.nomads.clear();
            }
        } else if !self.immigration(e).is_some_and(|i| i.nomads.is_empty()) {
            return;
        }
        self.spawn_band(e);
    }

    /// `FUN_14004ebd0(x, y, target)`: up to 50 cells within the margin of `(x, y)` that are
    /// walkable and either by water or in the target's region.
    fn edge(&mut self, d: &ImmigrationTemplate, x: i32, y: i32, target: i32) -> Option<i32> {
        let n = self.size as i32;
        for _ in 0..50 {
            let cx = (x + self.rng.rand_tol(d.margin)).clamp(0, n - 1);
            let cy = (y + self.rng.rand_tol(d.margin)).clamp(0, n - 1);
            let cell = cy * n + cx;
            let a = 1u32 << (self.cells.types[cell as usize].0 & 0x1f);
            if a & MASK == 0 {
                continue;
            }
            let b = 1u32 << (self.cells.types[target as usize].0 & 0x1f);
            if (a | b) & 0x1e0 != 0 {
                return Some(cell);
            }
            let (ra, rb) = (self.regions[cell as usize], self.regions[target as usize]);
            if ra != 0 && ra == rb {
                return Some(cell);
            }
        }
        None
    }

    /// `FUN_14004e540`: a band from a random map edge (ten tries at a reachable cell): `Rand(2 ×
    /// empty family slots) + population × share`, at least two, each walking from near that
    /// edge to one of the town hall's create points.
    fn spawn_band(&mut self, e: usize) {
        let Some(d) = self.immigration_desc(e).cloned() else {
            return;
        };
        let n = self.size as i32;
        let target = self.cell_of(self.use_point(e, 0).0);
        let mut tries = 0;
        let (x, y, start) = loop {
            let side = self.rng.rand_below(4);
            let (x, y) = if side < 2 {
                let x = if side == 0 {
                    d.margin
                } else {
                    n - d.margin - 1
                };
                (x, self.rng.rand_below(n as u32) as i32)
            } else {
                let x = self.rng.rand_below(n as u32) as i32;
                (
                    x,
                    if side == 2 {
                        d.margin
                    } else {
                        n - d.margin - 1
                    },
                )
            };
            if let Some(start) = self.edge(&d, x, y, target) {
                break (x, y, start);
            }
            tries += 1;
            if 9 < tries {
                return;
            }
        };
        let population = self.population;
        let f = self.rng.rand_float();
        let free: i32 = self
            .workforce
            .residences
            .iter()
            .filter_map(|r| self.homes.get(r))
            .map(|h| h.families.iter().filter(|f| f.is_empty()).count() as i32)
            .sum();
        let mut count = self.rng.rand_below((free * 2) as u32) as i32
            + (population as f32 * (f * (d.share.1 - d.share.0) + d.share.0)) as i32;
        if count < 2 {
            count = 2;
        }
        let points: usize = self
            .interact_entities(e)
            .iter()
            .map(|&x| self.templates[self.placed[x].template].create_points.len())
            .sum();
        for _ in 0..count {
            let cell = self.edge(&d, x, y, target).unwrap_or(start);
            let k = self.rng.rand_below(points as u32) as usize;
            let dest = self.create_point(e, k);
            let ground = self.ground_at(cell);
            if let Some(id) = self.add_nomad(d.nomad, e, ground, dest) {
                if let Some(i) = self.immigration(e) {
                    i.nomads.push(id);
                }
            }
        }
        if let Some(i) = self.immigration(e) {
            i.arrived = 0;
        }
    }

    /// `FUN_1401282f0(interact, 2, k)`: create point `k` (the entity's own, then its parts').
    fn create_point(&self, e: usize, mut k: usize) -> [f32; 3] {
        for x in self.interact_entities(e) {
            let points = &self.templates[self.placed[x].template].create_points;
            if k < points.len() {
                return self.world_point(x, points[k]);
            }
            k -= points.len();
        }
        self.world_point(e, [0.0; 3])
    }

    /// The nomad entity (`FUN_1400c8bf0`) and its setup (`FUN_140050990`): `Rand(2)` its
    /// variant, placed at `spawn` turned `RandFloatTol × 180`; activation runs state 0 (walk to
    /// `target`).
    fn add_nomad(
        &mut self,
        template: usize,
        town_hall: usize,
        spawn: [f32; 3],
        target: [f32; 3],
    ) -> Option<usize> {
        let kind = self.wildlife.nomad_kinds.get(&template)?.clone();
        let variant = self.rng.rand_below(2) as usize;
        let yaw = self.rng.rand_float_tol() * 180.0;
        let id = self.placed.len();
        let t = &self.templates[template];
        self.placed.push(crate::seeding::Placed {
            template,
            bucket: t.bucket,
            cell: (
                self.cell_of(spawn) % self.size as i32,
                self.cell_of(spawn) / self.size as i32,
            ),
            footprint: (1, 1),
            position: spawn,
            yaw,
            variant,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        let mut character = Character::new(kind.character, spawn, 1.0);
        character.yaw = yaw;
        self.wildlife.nomads.insert(
            id,
            Nomad {
                template,
                character,
                state: 0,
                town_hall: Some(town_hall),
                spawn,
                target,
                timer: 0,
                variant,
            },
        );
        self.nomad_walk(id, target);
        *self.models_dirty = true;
        Some(id)
    }

    /// `FUN_1400504c0`: the nomad drops what it does and walks to `to`.
    fn nomad_walk(&mut self, id: usize, to: [f32; 3]) {
        let Some(nomad) = self.wildlife.nomads.get(&id) else {
            return;
        };
        let Some(group) = self
            .wildlife
            .groups
            .get(&(nomad.template, nomad.variant))
            .cloned()
        else {
            return;
        };
        let walk = MoveTo::new(to, 1, 0, 0xe1f, 0, -1.0, self.rng);
        let ch = &mut self.wildlife.nomads.get_mut(&id).unwrap().character;
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, &group, &mut []);
        stack.push(ch, &group, Action::move_to(walk));
        ch.stack = stack;
    }

    fn remove_nomad(&mut self, id: usize) {
        if self.wildlife.nomads.remove(&id).is_some() {
            self.placed[id].alive = false;
            *self.models_dirty = true;
        }
    }

    /// The nomads' StateMachine updates: 0 at the town hall, it tells it (message 6) and
    /// waits (`FUN_140050320`); 1 waiting, it wanders now and then (`FUN_14004ff60`); 2 back
    /// where it came from, it is gone (`FUN_14004fe90`).
    pub fn update_nomads(&mut self, dt: i64) {
        let ids: Vec<usize> = self.wildlife.nomads.keys().copied().collect();
        for id in ids {
            let Some(nomad) = self.wildlife.nomads.get(&id) else {
                continue;
            };
            if nomad.character.stack.is_active() {
                continue;
            }
            match nomad.state {
                0 => match nomad.town_hall.filter(|t| self.sites.contains_key(t)) {
                    None => self.remove_nomad(id),
                    Some(t) => {
                        self.immigration_message(t, 6);
                        if let Some(n) = self.wildlife.nomads.get_mut(&id) {
                            n.state = 1;
                        }
                    }
                },
                1 => self.nomad_wander(id, dt),
                _ => self.remove_nomad(id),
            }
        }
    }

    /// `FUN_14004ff60`: every 30 ± 15 s, a ground cell (types 0..2) of the 16 × 16 around
    /// where it waits, a third of a cell off its centre.
    fn nomad_wander(&mut self, id: usize, dt: i64) {
        let n = self.size as i32;
        let nomad = self.wildlife.nomads.get_mut(&id).unwrap();
        nomad.timer -= dt;
        if 0 <= nomad.timer {
            return;
        }
        let target = nomad.target;
        let t = self.rng.rand_float_tol() * 15.0 + 30.0;
        self.wildlife.nomads.get_mut(&id).unwrap().timer = ticks(t);
        let c = self.cell_of(target);
        let (x0, y0) = (c % n - 8, c / n - 8);
        let mut cells = Vec::new();
        for y in y0..y0 + 16 {
            for x in x0..x0 + 16 {
                if 0 <= x
                    && x < n
                    && 0 <= y
                    && y < n
                    && self.cells.types[(y * n + x) as usize].0 < 3
                {
                    cells.push(y * n + x);
                }
            }
        }
        if cells.is_empty() {
            return;
        }
        let k = self.rng.rand_below(cells.len() as u32) as usize;
        let dy = self.rng.rand_float_tol() * 0.33;
        let dx = self.rng.rand_float_tol() * 0.33;
        let g = self.ground_at(cells[k]);
        self.nomad_walk(id, [g[0] + dx, g[1] + dy, g[2]]);
    }

    /// A nomad's state message: 1 (`FUN_14004fe00`, in 0 and 1) it leaves for where it came
    /// from and forgets the town hall; 0 (`FUN_14004fd80`, in 1) it is gone.
    fn nomad_message(&mut self, id: usize, msg: i32) {
        let Some(nomad) = self.wildlife.nomads.get_mut(&id) else {
            return;
        };
        match (nomad.state, msg) {
            (0 | 1, 1) => {
                nomad.state = 2;
                let spawn = nomad.spawn;
                self.nomad_walk(id, spawn);
                if let Some(n) = self.wildlife.nomads.get_mut(&id) {
                    n.town_hall = None;
                }
            }
            (1, 0) => self.remove_nomad(id),
            _ => {}
        }
    }

    /// `FUN_14004dc60`: the band leaves (message 1 to each).
    pub(crate) fn send_band_away(&mut self, e: usize) {
        let band = self
            .immigration(e)
            .map(|i| std::mem::take(&mut i.nomads))
            .unwrap_or_default();
        for id in band {
            self.nomad_message(id, 1);
        }
        if let Some(i) = self.immigration(e) {
            i.arrived = 0;
        }
    }

    /// State 0x11 enter (`FUN_14004d810`): event 14 at severity 2.
    pub(crate) fn band_arrived(&mut self, e: usize) {
        self.site_events.push((ARRIVED, 2, e));
    }

    /// State 0x11 update (`FUN_14004d760`): from the leaving month the band goes.
    pub(crate) fn band_waiting(&mut self, e: usize) {
        let Some(d) = self.immigration_desc(e) else {
            return;
        };
        if d.leave <= self.calendar_month() {
            self.send_band_away(e);
            self.machine(e).request(2);
        }
    }

    /// The ImmigrationComponent's state messages: 6 a nomad arrived (`FUN_14004d950`, in
    /// 0x10); 8 turned away (`FUN_14004d680`, in 0x11). Message 7 (taking them in,
    /// `FUN_14004d6f0`) needs the citizens: [`crate::agents::Agents::admit_band`].
    pub fn immigration_message(&mut self, e: usize, msg: i32) {
        if self.immigration(e).is_none() || self.machine(e).index < 0 {
            return;
        }
        match (self.machine(e).state, msg) {
            (0x10, 6) => {
                let i = self.immigration(e).unwrap();
                i.arrived += 1;
                if i.arrived == i.nomads.len() as i32 {
                    self.machine(e).request(0x11);
                }
            }
            (0x11, 8) => {
                self.send_band_away(e);
                self.machine(e).request(2);
            }
            _ => {}
        }
    }

    /// `FUN_14004ddb0`'s town side: the visitors' disease risk (the larger of the town's and
    /// `+0x60`, for the longer of the town's and `+0x64` months) and the band (each nomad and
    /// where it stands) for the citizens to be made. `None` unless the band is waiting (0x11).
    pub fn take_band(&mut self, e: usize) -> Option<Vec<(usize, [f32; 3])>> {
        if self.machine(e).index < 0 || self.machine(e).state != 0x11 {
            return None;
        }
        let d = self.immigration_desc(e)?.clone();
        let band = self.immigration(e)?.nomads.clone();
        if self.visitors.0 < d.risk {
            self.visitors.0 = d.risk;
        }
        let time = ticks(d.risk_months as f32 * PERIOD);
        if self.visitors.1 < time {
            self.visitors.1 = time;
        }
        Some(
            band.iter()
                .filter_map(|&id| Some((id, self.wildlife.nomads.get(&id)?.character.position)))
                .collect(),
        )
    }

    /// After the citizens are made: each nomad is gone (message 0) and the town hall goes back
    /// to 2.
    pub fn band_taken(&mut self, e: usize) {
        let band = self
            .immigration(e)
            .map(|i| std::mem::take(&mut i.nomads))
            .unwrap_or_default();
        for id in band {
            self.nomad_message(id, 0);
        }
        if let Some(i) = self.immigration(e) {
            i.arrived = 0;
        }
        self.machine(e).request(2);
    }
}
