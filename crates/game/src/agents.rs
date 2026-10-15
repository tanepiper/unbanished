//! The citizens' CharacterComponent and CitizenAIComponent run together each simulation
//! iteration. See docs/CITIZEN-AI.md.
//!
//! Order per iteration: animations advance (`AnimationManager::Update`), every character runs
//! its actions and moves (entity manager bit-`0x04` set), "sequence finished" messages re-run
//! the AI with zero time (`FUN_140114810`), then the AI updates.
//!
//! Deviations: the AI runs every iteration instead of on its bucket's quota, and after the
//! placed entities' bucket pass instead of interleaved with it.

use banished_formats::game::CharacterDescription;
use banished_formats::types::AnimationGroup;

use crate::activities::{loiter, EntityView, StorageGeometry, TownView};
use crate::ai::{activity, CitizenAi, CitizenView, Thresholds};
use crate::character::{Action, Character, Component, Message, MoveTo, World};
use crate::citizens::Citizen;
use crate::pathfind::{regions, Pathfinder, WALKABLE};
use crate::random::Random;
use crate::scheduler::ticks;
use crate::sectors::SectorGraph;
use crate::seeding::{Cells, Placed, Template};
use crate::stock::{find_storage, RawMaterial, Storage, StorageSite};
use crate::terrain::{CellType, HeightGrid};

/// `DAT_1401788fc`: cooldown set by the fetch-tool and fetch-clothing activities.
const FETCH_COOLDOWN: f32 = 75.0;

/// The CitizenDescription values the AI reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct CitizenRulesAi {
    pub thresholds: Thresholds,
    /// `+0x6c` starting health, `+0xa4` starting happiness.
    pub health: f32,
    pub happiness: f32,
    /// `+0xbc`: loiter search range.
    pub loiter_range: i32,
    /// CitizenDescription +0x130 multiplied by 300 (FUN_140117730).
    pub clothing_duration: f32,
    pub health_max: f32,
    /// FUN_140118340: midpoint for food, quarter-point for cold status warnings.
    pub hunger_limit: f32,
    pub cold_limit: f32,
    pub profession_allows_idle: bool,
}

/// Citizen fields read/written by equipment activities and Citizen messages 1/6/7.
#[derive(Debug, Clone, Default)]
pub struct CitizenState {
    pub health: f32,
    pub happiness: f32,
    pub hunger: f32,
    pub cold: f32,
    pub sick: bool,
    pub work: Option<usize>,
    /// StatusIconComponent +0xf0 (Citizen +0x280 points to that component).
    pub status: u32,
    // Not ported: tool use during jobs and the citizen clothing-wear countdown.
    pub tool_status: i32,        // +0xa8
    pub tool_uses: i32,          // +0xac
    pub clothing_status: i32,    // +0x1e0
    pub clothing: i32,           // +0x1e4
    pub clothing_remaining: i64, // +0x1e8
    pub health_events: u32,      // +0x54
    pub flags: u32,              // +0x88
    pub supply_activity: i32,    // +0xb0
    pub job: i32,                // +0x30
    pub home: Option<usize>,     // +0x238
    pub carried: Storage,
}

impl CitizenState {
    /// Port of FUN_140117730 / FUN_1401152b0 (equipment message branches).
    pub fn message(&mut self, m: Message, rules: CitizenRulesAi) {
        match m.message {
            1 => {
                crate::status::set_icon(&mut self.status, 4, false, false);
                self.tool_uses = self.tool_uses.wrapping_add(m.a);
                self.tool_status = m.a;
            }
            6 | 7 => {
                self.clothing = m.message - 5;
                self.clothing_status = self.clothing;
                self.clothing_remaining = ticks(rules.clothing_duration);
                self.flags |= 0x800;
            }
            8 => self.flags |= 0x40000,
            _ => {}
        }
    }
}

struct TownStock<'a> {
    materials: &'a [RawMaterial],
    storages: &'a mut [Storage],
    sites: &'a [StorageSite],
}

pub struct Agents {
    pub characters: Vec<Character>,
    pub ais: Vec<CitizenAi>,
    pub states: Vec<CitizenState>,
    sectors: SectorGraph,
    pathfinder: Pathfinder,
    regions: Vec<u16>,
    region_types: Vec<CellType>,
    rules: CitizenRulesAi,
}

impl Agents {
    pub fn new(
        citizens: &[Citizen],
        cells: &Cells,
        desc: CharacterDescription,
        rules: CitizenRulesAi,
        carried: Storage,
        scale: impl Fn(&Citizen) -> f32,
    ) -> Agents {
        Agents {
            characters: citizens
                .iter()
                .map(|c| Character::new(desc, c.position, scale(c)))
                .collect(),
            ais: citizens.iter().map(|_| CitizenAi::default()).collect(),
            states: citizens
                .iter()
                .map(|_| CitizenState {
                    job: -1,
                    health: rules.health,
                    happiness: rules.happiness,
                    carried: carried.clone(),
                    ..Default::default()
                })
                .collect(),
            sectors: SectorGraph::new(cells.size, &cells.types),
            pathfinder: Pathfinder::new(cells.size, &cells.types),
            regions: regions(cells.size, &cells.types),
            region_types: cells.types.clone(),
            rules,
        }
    }

    pub fn rules(&self) -> CitizenRulesAi {
        self.rules
    }

    /// `FUN_1400a1950` when the map changed, and the path grid's type commands.
    fn sync_map(&mut self, cells: &Cells) {
        if self.region_types == cells.types {
            return;
        }
        for (i, (old, new)) in self.region_types.iter().zip(&cells.types).enumerate() {
            if old != new {
                self.pathfinder.set_type(i, *new);
                self.sectors
                    .mark((i % cells.size) as i32, (i / cells.size) as i32);
            }
        }
        self.region_types = cells.types.clone();
        self.regions = regions(cells.size, &cells.types);
    }

    /// One simulation iteration of every citizen. `groups[i]` is citizen `i`'s animation group.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        citizens: &mut [Citizen],
        cells: &Cells,
        placed: &[Placed],
        templates: &[Template],
        terrain: &HeightGrid,
        groups: &[Option<&AnimationGroup>],
        materials: &[RawMaterial],
        storages: &mut [Storage],
        sites: &[StorageSite],
        rng: &mut Random,
        dt: f32,
    ) {
        self.sync_map(cells);
        // FUN_140118340's icon writes belong to simulation, never layout/hover/draw.
        // The surrounding needs/assignment updates remain unported; reflect their live fields.
        for state in &mut self.states {
            for (bit, on) in [
                (0, state.home.is_none()),
                (
                    1,
                    state.hunger
                        > self.rules.thresholds.hungry_urgent
                            + (self.rules.hunger_limit - self.rules.thresholds.hungry_urgent) * 0.5,
                ),
                (
                    2,
                    state.cold
                        > self.rules.thresholds.cold_urgent
                            + (self.rules.cold_limit - self.rules.thresholds.cold_urgent) * 0.25,
                ),
                (
                    3,
                    !self.rules.profession_allows_idle && state.work.is_none(),
                ),
                (5, state.sick),
            ] {
                crate::status::set_icon(&mut state.status, bit, on, false);
            }
        }
        self.sectors.age_cache(ticks(dt));
        let mut stock = TownStock {
            materials,
            storages,
            sites,
        };
        let entities = entity_views(placed, templates);
        for (ch, group) in self.characters.iter_mut().zip(groups) {
            if let Some(g) = group {
                ch.animate(g, dt);
            }
        }
        for i in 0..self.characters.len() {
            let Some(group) = groups[i] else { continue };
            let mut messages = Vec::new();
            {
                let mut world = World {
                    size: cells.size,
                    types: &cells.types,
                    regions: &self.regions,
                    terrain,
                    pathfinder: &mut self.pathfinder,
                    rng,
                    messages: &mut messages,
                    materials: stock.materials,
                    storages: stock.storages,
                    carried: &mut self.states[i].carried,
                };
                self.characters[i].update(&mut world, group, dt);
            }
            citizens[i].position = self.characters[i].position;
            for m in messages {
                if m.component == Component::Citizen {
                    self.states[i].message(m, self.rules);
                }
                if m.component == Component::CitizenAi && m.message == 0 {
                    self.think(
                        i, citizens, cells, &entities, terrain, group, &mut stock, rng, 0,
                    );
                }
            }
        }
        for i in 0..self.ais.len() {
            let Some(group) = groups[i] else { continue };
            self.think(
                i,
                citizens,
                cells,
                &entities,
                terrain,
                group,
                &mut stock,
                rng,
                ticks(dt),
            );
        }
    }

    /// `FUN_1401148b0` for citizen `i`.
    #[allow(clippy::too_many_arguments)]
    fn think(
        &mut self,
        i: usize,
        citizens: &[Citizen],
        cells: &Cells,
        entities: &[EntityView],
        terrain: &HeightGrid,
        group: &AnimationGroup,
        stock: &mut TownStock,
        rng: &mut Random,
        dt: i64,
    ) {
        let c = &citizens[i];
        let view = CitizenView {
            age: c.age,
            school: c.school.is_some(),
            tool: self.states[i].tool_uses,
            clothing: self.states[i].clothing,
            home: self.states[i].home.is_some(),
            carrying: self.states[i].carried.used(stock.materials),
            health: self.states[i].health,
            happiness: self.states[i].happiness,
            hunger: self.states[i].hunger,
            cold: self.states[i].cold,
            sick: self.states[i].sick,
            ..Default::default()
        };
        let rules = self.rules;
        let mut ai = std::mem::take(&mut self.ais[i]);
        let busy = self.characters[i].stack.is_active();
        let size = cells.size;
        let town = TownView {
            size,
            types: &cells.types,
            regions: &self.regions,
            occupants: &cells.occupants,
            objects: &cells.objects,
            entities,
            happiness: &[],
        };
        let ch = &mut self.characters[i];
        ai.update(
            dt,
            &view,
            &rules.thresholds,
            rng,
            || busy,
            |ai, a, rng| {
                run(
                    ai,
                    a,
                    ch,
                    &mut self.states[i],
                    &town,
                    &mut self.sectors,
                    stock,
                    terrain,
                    group,
                    rules,
                    size,
                    rng,
                )
            },
        );
        self.ais[i] = ai;
    }
}

/// `FUN_1401146c0`'s targets that are ported; the rest report "nothing to do".
#[allow(clippy::too_many_arguments)]
fn run(
    ai: &mut CitizenAi,
    a: u32,
    ch: &mut Character,
    state: &mut CitizenState,
    town: &TownView,
    sectors: &mut SectorGraph,
    stock: &mut TownStock,
    terrain: &HeightGrid,
    group: &AnimationGroup,
    rules: CitizenRulesAi,
    size: usize,
    rng: &mut Random,
) -> bool {
    match a {
        activity::TOOL | activity::CLOTHING => {
            // FUN_140111a50 / FUN_140111830: cooldown is set even when already equipped.
            ai.cooldowns[a as usize] = ticks(FETCH_COOLDOWN);
            let tool = a == activity::TOOL;
            if if tool {
                state.tool_uses > 0
            } else {
                state.clothing != 0
            } {
                return false;
            }
            let cell = town.cell_of(ch.position);
            let home = state.home.map_or(-1, |h| town.entrance(h, rng));
            let ground = |c| cell_ground(size, terrain, c);
            let mut geometry = StorageGeometry {
                town,
                sectors,
                ground: &ground,
            };
            let Some(found) = find_storage(
                stock.sites,
                stock.storages,
                stock.materials,
                if tool { 2 } else { 4 },
                cell,
                home,
                &mut geometry,
                rng,
            ) else {
                if tool {
                    state.tool_status = 0;
                    crate::status::set_icon(&mut state.status, 4, true, false);
                } else {
                    state.clothing_status = 0;
                }
                state.flags |= 4;
                // Not ported: drawing the StatusIconComponent's billboard (its mask is live).
                return false;
            };
            // FUN_1400c2540 / FUN_1400c2180: push in reverse execution order.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, stock.storages);
            stack.set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
            let material = &stock.materials[found.material];
            stack.push(
                ch,
                group,
                Action::SendMessage(Message {
                    component: Component::Citizen,
                    message: if tool {
                        1
                    } else {
                        6 + ((material.flags >> 15) & 1) as i32
                    },
                    entity: None,
                    a: if tool { material.tool_uses } else { 0 },
                    b: 0,
                }),
            );
            stock.storages[found.storage].reserve(found.material, 1);
            stack.push(
                ch,
                group,
                Action::ConsumeStoredRawMaterial {
                    storage: found.storage,
                    items: vec![(found.material, 1)],
                    reserved: true,
                },
            );
            // FUN_1400c7e70: face the source model (or map rectangle centre).
            let e = &town.entities[stock.sites[found.site].entity];
            let centre = if e.has_model {
                e.position
            } else {
                e.rect.map_or(e.position, |(x, y, w, h)| {
                    cell_ground(size, terrain, (y + (h >> 1)) * size as i32 + x + (w >> 1))
                })
            };
            let facing = crate::character::work_facing(centre, found.target);
            stack.push(
                ch,
                group,
                Action::PerformWork {
                    facing,
                    anim: 8,
                    duration: ticks(1.0),
                    elapsed: 0,
                    consume: false,
                },
            );
            stack.push(
                ch,
                group,
                Action::move_to(MoveTo::new(found.target, 1, 0, WALKABLE, 0, -1.0, rng)),
            );
            ch.stack = stack;
            state.health_events |= 4;
            state.supply_activity = 0;
            state.job = -1;
            ch.stack.is_active()
        }

        activity::IDLE => {
            let cell = town.cell_of(ch.position);
            let ground = |c: i32| {
                let n = size as i32;
                let h = (n >> 1) as f32 - 0.5;
                let (x, y) = ((c % n) as f32 - h, (c / n) as f32 - h);
                let mut normal = [0.0; 3];
                [x, y, terrain.ground(x, y, &mut normal)]
            };
            let Some(plan) = loiter(town, cell, state.home, rules.loiter_range, rng, ground) else {
                return false;
            };
            // `FUN_1400bfb80`.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, stock.storages);
            stack.set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
            if let Some(e) = plan.happiness {
                // Citizen message 8 (state bit 0x40000), then HappinessComponent message 0.
                for (component, message, entity) in [
                    (Component::Citizen, 8, None),
                    (Component::Happiness, 0, Some(e)),
                ] {
                    stack.push(
                        ch,
                        group,
                        Action::SendMessage(Message {
                            component,
                            message,
                            entity,
                            a: 0,
                            b: 0,
                        }),
                    );
                }
            }
            stack.push(
                ch,
                group,
                Action::PlayAnimation {
                    anim: 0,
                    duration: ticks(plan.duration),
                    elapsed: 0,
                },
            );
            let walk = MoveTo::new(plan.target, 1, 0, WALKABLE, 0, -1.0, rng);
            stack.push(ch, group, Action::move_to(walk));
            ch.stack = stack;
            ai.since_loiter = 0;
            ch.stack.is_active()
        }
        _ => false,
    }
}

/// The parts of each placed entity the activities read.
pub fn entity_views(placed: &[Placed], templates: &[Template]) -> Vec<EntityView> {
    placed
        .iter()
        .map(|p| {
            let t = &templates[p.template];
            let rect = t.map.as_ref().map(|_| {
                let (w, h) = p.footprint;
                (p.cell.0, p.cell.1, w, h)
            });
            let (s, c) = p.yaw.to_radians().sin_cos();
            let use_points = t
                .use_points
                .iter()
                .map(|l| {
                    [
                        p.position[0] + c * l[0] - s * l[1],
                        p.position[1] + s * l[0] + c * l[1],
                        p.position[2] + l[2],
                    ]
                })
                .collect();
            EntityView {
                alive: p.alive,
                rect: if p.alive { rect } else { None },
                use_points,
                has_interact: t.has_interact,
                position: p.position,
                has_model: t.model.is_some(),
                happiness: None,
            }
        })
        .collect()
}

/// FUN_1400a48b0 without bridge picking (same ground query as MoveTo).
fn cell_ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}
