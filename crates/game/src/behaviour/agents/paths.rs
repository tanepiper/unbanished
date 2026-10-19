//! Paths for citizens: road and walking paths and the commute.

use super::*;

impl Agents {
    /// The cells of a road from `a` to `b`, over `mask`, avoiding the cells of zones. The road
    /// tool uses it.
    ///
    /// Original: `FUN_1400a22d0`.
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

    /// A citizen's walk from `a` to `b` as the path manager finds it (mask `0xe1f`, mode 0), for
    /// the Path tool; the finder's state is kept.
    ///
    /// Original: `FUN_1400a2480`, `FUN_1400e2c40`.
    pub fn walk_path(&mut self, cells: &Cells, a: i32, b: i32) -> Option<Vec<i32>> {
        self.sync_map(cells);
        let regions = &self.regions;
        self.pathfinder.keeping_state(|f| {
            f.set_mode(0);
            crate::pathfind::request(f, &cells.types, regions, crate::pathfind::WALKABLE, a, b)
        })
    }

    /// StockManager::Update's commute optimisation (see [`crate::behaviour::commute`]) by `dt`
    /// ticks.
    ///
    /// Original: `FUN_140091780`.
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
