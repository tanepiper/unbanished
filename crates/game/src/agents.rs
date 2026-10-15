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

use crate::activities::{loiter, EntityView, TownView};
use crate::ai::{activity, CitizenAi, CitizenView, Thresholds};
use crate::character::{Action, Character, Component, Message, MoveTo, World};
use crate::citizens::Citizen;
use crate::pathfind::{regions, Pathfinder, WALKABLE};
use crate::random::Random;
use crate::scheduler::ticks;
use crate::seeding::{Cells, Placed, Template};
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
}

pub struct Agents {
    pub characters: Vec<Character>,
    pub ais: Vec<CitizenAi>,
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
        scale: impl Fn(&Citizen) -> f32,
    ) -> Agents {
        Agents {
            characters: citizens
                .iter()
                .map(|c| Character::new(desc, c.position, scale(c)))
                .collect(),
            ais: citizens.iter().map(|_| CitizenAi::default()).collect(),
            pathfinder: Pathfinder::new(cells.size, &cells.types),
            regions: regions(cells.size, &cells.types),
            region_types: cells.types.clone(),
            rules,
        }
    }

    /// `FUN_1400a1950` when the map changed, and the path grid's type commands.
    fn sync_map(&mut self, cells: &Cells) {
        if self.region_types == cells.types {
            return;
        }
        for (i, (old, new)) in self.region_types.iter().zip(&cells.types).enumerate() {
            if old != new {
                self.pathfinder.set_type(i, *new);
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
        rng: &mut Random,
        dt: f32,
    ) {
        self.sync_map(cells);
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
                };
                self.characters[i].update(&mut world, group, dt);
            }
            citizens[i].position = self.characters[i].position;
            for m in messages {
                if m.component == Component::CitizenAi && m.message == 0 {
                    self.think(i, citizens, cells, &entities, terrain, group, rng, 0);
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
        rng: &mut Random,
        dt: i64,
    ) {
        let c = &citizens[i];
        let view = CitizenView {
            age: c.age,
            school: c.school.is_some(),
            health: self.rules.health,
            happiness: self.rules.happiness,
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
            |ai, a, rng| run(ai, a, &view, ch, &town, terrain, group, rules, size, rng),
        );
        self.ais[i] = ai;
    }
}

/// `FUN_1401146c0`'s targets that are ported; the rest report "nothing to do".
#[allow(clippy::too_many_arguments)]
fn run(
    ai: &mut CitizenAi,
    a: u32,
    view: &CitizenView,
    ch: &mut Character,
    town: &TownView,
    terrain: &HeightGrid,
    group: &AnimationGroup,
    rules: CitizenRulesAi,
    size: usize,
    rng: &mut Random,
) -> bool {
    match a {
        activity::TOOL | activity::CLOTHING => {
            // `FUN_140111a50` / `FUN_140111830`: set the cooldown; the storage query and the
            // fetch sequence (`FUN_140093200`, `FUN_1400c2540` / `FUN_1400c2180`) are next.
            ai.cooldowns[a as usize] = ticks(FETCH_COOLDOWN);
            let _ = view;
            false
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
            let Some(plan) = loiter(town, cell, None, rules.loiter_range, rng, ground) else {
                return false;
            };
            // `FUN_1400bfb80`.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group);
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
                let (mut w, mut h) = t.size.unwrap_or((1, 1));
                if (p.yaw as i32).rem_euclid(180) == 90 {
                    std::mem::swap(&mut w, &mut h);
                }
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
                happiness: None,
            }
        })
        .collect()
}
