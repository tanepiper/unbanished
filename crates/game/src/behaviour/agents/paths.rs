//! Paths for citizens: road and walking paths and the commute.

use super::*;

impl Agents {
    /// `FUN_1400a22d0` for the road tool: a road's cells from `a` to `b` over `mask` (the
    /// zones' cells blocked).
    pub fn road_path(
        &mut self,
        cells: &Cells,
        a: i32,
        b: i32,
        mask: u32,
        manhattan: bool,
    ) -> Option<Vec<i32>> {
        self.sync_map(cells);
        let blocked: Vec<bool> = cells.occupants.iter().map(Option::is_some).collect();
        self.pathfinder.find_road(a, b, mask, manhattan, &blocked)
    }

    /// A citizen's walk from `a` to `b` as the path manager finds it (`FUN_1400a2480`, mask
    /// `0xe1f`, mode 0), for the Path tool (`FUN_1400e2c40`); the finder's state is kept.
    pub fn walk_path(&mut self, cells: &Cells, a: i32, b: i32) -> Option<Vec<i32>> {
        self.sync_map(cells);
        let regions = &self.regions;
        self.pathfinder.keeping_state(|f| {
            f.set_mode(0);
            crate::pathfind::request(f, &cells.types, regions, crate::pathfind::WALKABLE, a, b)
        })
    }

    /// StockManager::Update's commute optimisation (`FUN_140091780`, see
    /// [`crate::behaviour::commute`]) by `dt` ticks.
    pub fn update_commute(
        &mut self,
        citizens: &[Citizen],
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        dt: i64,
    ) {
        self.sync_map(env.cells);
        let mut job = std::mem::take(&mut env.construction.commute);
        let entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        let (mut sim, _) = self.sim(citizens, env, groups, &adult, &entities);
        sim.update_commute(&mut job, citizens, dt);
        env.construction.commute = job;
    }
}
