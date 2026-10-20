//! One simulation iteration: commands, the town's update, terrain changes, the calendar, the
//! environment, then the stock, the jobs and the statistics.

use unbanished_formats::pkg::Archive;
use unbanished_game::scheduler::ticks;
use unbanished_game::work::tracker::CitizenSample;

use super::Simulation;
use crate::timing;

/// What an iteration did that the game app shows.
#[derive(Debug, Default)]
pub struct StepEvents {
    /// Spawning or removal changed the model instances.
    pub models_changed: bool,
    /// 3D effects to play, at their entities' positions, in order.
    pub sounds: Vec<([f32; 3], String)>,
    /// The heights changed (tunnels, flattening): the drawn terrain follows.
    pub reshaped: bool,
}

impl Simulation {
    /// One iteration: the entity scheduler, the integer calendar, then the environment.
    /// Growth reads the prior environment temperature; weather draws follow entity callbacks.
    ///
    /// Original: `FUN_14009b850`.
    pub fn iterate(&mut self, archive: &Archive, dt: f32) -> StepEvents {
        let mut out = StepEvents::default();
        // Player commands and placed sites take effect between iterations.
        self.apply_commands();
        if let (Some(town), Some(heights)) = (&mut self.town, &self.heights) {
            let grid = heights.grid();
            let temperature = self.environment.as_ref().map_or(0.0, |e| e.temperature);
            let changed = {
                let _p = timing::scope("sim.town");
                town.iterate(&grid, &mut self.rng, dt, temperature)
            };
            // AnimationManager::Update runs inside the same iteration (FUN_14009b850).
            let stock = std::array::from_fn(|i| {
                (self.stock_warnings.count[i], self.stock_warnings.limits[i])
            });
            let changed = {
                let _p = timing::scope("sim.citizens");
                town.update_citizens(&grid, &mut self.rng, dt, temperature, stock)
            } | changed;
            // AnimAudioEvents that passed their draw: `PlayEffect3D(effect, 4)` at the entity.
            for (entity, effect) in town.animation_events.drain(..) {
                out.sounds.push((town.placed[entity].position, effect));
            }
            out.models_changed = changed;
        }
        // The tunnels' terraforming and the flattening under stamped footprints and roads, on
        // the terrain the town reads (`FUN_140057c70`, `FUN_1400a35b0`).
        if let (Some(town), Some(heights)) = (&mut self.town, &mut self.heights) {
            let c = &town.construction;
            if !c.terraform_requests.is_empty() || !c.flatten_requests.is_empty() {
                let width = heights.width();
                out.reshaped |= town.apply_terraforms(&mut heights.values, width);
            }
        }
        self.calendar.advance(ticks(dt));
        let mut tornado = false;
        if let Some(env) = &mut self.environment {
            tornado = env.update(self.calendar.seconds(), dt, &mut self.rng);
        }
        // `FUN_1400999b0` step 6: the environment template's entity.
        if tornado {
            if let (Some(town), Some(heights), Some(reference)) = (
                &mut self.town,
                &self.heights,
                self.environment_template.as_deref(),
            ) {
                let grid = heights.grid();
                town.spawn_tornado(archive, reference, &grid, &mut self.rng);
                out.models_changed = true;
            }
        }
        // `FUN_140091780`: recompute stock (`FUN_14008f3a0`), then its warnings
        // (`FUN_1400915d0`, posted through `FUN_14009aa50` at severity 3).
        if let Some(town) = &mut self.town {
            self.stock_warnings
                .recompute(&town.registered_storages(), &town.materials);
            // StockManager::Update's commute optimisation (every 300 s).
            if let Some(heights) = &self.heights {
                let grid = heights.grid();
                town.update_commute(&grid, &mut self.rng, dt);
            }
            // The job manager's pass and clock (`FUN_14009b850`, after the stock update).
            town.construction.jobs.step(ticks(dt));
            // `FUN_14008fc00` every 75 s: the citizens and every stock slot.
            let life = &town.agents.life;
            let samples: Vec<CitizenSample> = town
                .citizens
                .iter()
                .zip(&town.agents.states)
                .filter(|(c, _)| c.alive)
                .map(|(c, s)| CitizenSample {
                    adult: life.adult_age() <= c.age,
                    school: s.school.is_some(),
                    health: s.health,
                    happiness: s.happiness,
                    educated: life.i(0x118) <= c.education,
                    coat: s.clothing != 0,
                })
                .collect();
            let (health_max, happiness_max) = (life.f(0x70), life.f(0xa4));
            self.statistics.update(
                ticks(dt),
                &samples,
                health_max,
                happiness_max,
                &self.stock_warnings.count,
                &mut town.construction.counters,
            );
            let (month, year) = (self.month(), self.calendar.year as i32);
            for kind in self.stock_warnings.check(ticks(dt)) {
                // AutoSlow is applied once the frame's events are in (`slow_request`).
                self.events
                    .add(kind, 3, -1, month, year, Default::default(), true);
            }
        }
        out
    }
}
