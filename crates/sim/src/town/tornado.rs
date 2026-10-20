//! The environment's tornado (made in step 6 of the environment update from the
//! EnvironmentDescription's template): the entity and its TornadoComponent's setup. See
//! docs/TORNADO.md.
//!
//! Original: `FUN_1400c9d70`, `FUN_140058f00`, `FUN_1400999b0`, `FUN_1400c8bf0`.

use unbanished_formats::pkg::Archive;
use unbanished_game::disasters::tornado::Tornado;
use unbanished_game::nature::seeding::Placed;
use unbanished_game::random::Random;
use unbanished_game::terrain::HeightGrid;

use super::StartingTown;

/// Event type 30: "A tornado has touched down near the town!" (severity 3).
pub const TORNADO_EVENT: i32 = 0x1e;

impl StartingTown {
    /// A tornado of `reference` (`Template\Tornado.rsc:resource`) forms: its path is drawn
    /// from the game's Random and the event posted at its start. Returns the entity.
    pub fn spawn_tornado(
        &mut self,
        archive: &Archive,
        reference: &str,
        terrain: &HeightGrid,
        rng: &mut Random,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let t = self.templates[template].tornado?;
        let size = self.cells.size;
        let tornado = Tornado::new(&t, size, terrain, &self.cells.floors, rng);
        let n = size as i32;
        let half = (n >> 1) as f32;
        let p = tornado.position;
        let cell = (
            ((p[0] + half) as i32).clamp(0, n - 1),
            ((p[1] + half) as i32).clamp(0, n - 1),
        );
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell,
            footprint: (1, 1),
            position: p,
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        self.construction.tornadoes.insert(id, tornado);
        self.construction.site_events.push((TORNADO_EVENT, 3, id));
        log::info!("a tornado forms at {cell:?}");
        Some(id)
    }
}
