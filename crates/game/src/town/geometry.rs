//! Where things are: cells, ground heights, reachability, path ranks and the points citizens work from.

use super::*;

impl Sim<'_, '_> {
    /// [`floor_ground`] on this map.
    pub(crate) fn ground_at(&self, cell: i32) -> [f32; 3] {
        floor_ground(&self.cells.floors, self.size, self.terrain, cell)
    }

    pub(crate) fn cell_of(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    pub(crate) fn citizen_cell(&self, c: usize) -> i32 {
        self.cell_of(self.characters[c].position)
    }

    /// `Interact::PointPosition(entity, 0)` as a cell.
    pub(crate) fn point_cell(&self, entity: usize) -> i32 {
        self.town().point_cell(entity)
    }

    /// `Map::PathRank` (`FUN_1400a4ce0`): mode 1 stores what it computes.
    pub(crate) fn path_rank(&mut self, a: i32, b: i32, store: bool) -> i32 {
        let reachable = self.reachable(a, b);
        self.sectors.rebuild(&self.cells.types);
        self.sectors
            .distance(a, b, reachable, store, &self.cells.types)
    }

    pub(super) fn town(&self) -> TownView<'_> {
        TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            floors: &self.cells.floors,
            entities: self.entities,
            happiness: self.happiness,
        }
    }

    pub(crate) fn reachable(&self, a: i32, b: i32) -> bool {
        self.town().reachable(WALKABLE, a, b)
    }

    /// `Map::Reachable` (`FUN_1400a55c0`) with another mask.
    pub(crate) fn reachable_by(&self, mask: u32, a: i32, b: i32) -> bool {
        self.town().reachable(mask, a, b)
    }

    /// The workforce with the workers and the workplace searches over the town; the workers'
    /// professions and workplaces are written back afterwards.
    pub(crate) fn with_search<R>(
        &mut self,
        f: impl FnOnce(
            &mut crate::work::professions::Workforce,
            &mut [crate::work::professions::Worker],
            &mut dyn crate::work::professions::Search,
        ) -> R,
    ) -> R {
        let mut workers = self.workers();
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            floors: &self.cells.floors,
            entities: self.entities,
            happiness: self.happiness,
        };
        let (size, characters) = (self.size as i32, &*self.characters);
        let cell = |c: usize| {
            let p = characters[c].position;
            let half = (size >> 1) as f32;
            let x = ((p[0] + half) as i32).clamp(0, size - 1);
            let y = ((p[1] + half) as i32).clamp(0, size - 1);
            size * y + x
        };
        let professions = self.professions;
        let anywhere = |p: usize| professions.works_anywhere(p);
        let sites = &*self.sites;
        let priority = |e: usize| {
            sites
                .get(&e)
                .and_then(|s| s.work_priority)
                .unwrap_or(i32::MAX)
        };
        let mut search = crate::behaviour::activities::TownSearch {
            town: &town,
            cell: &cell,
            anywhere: &anywhere,
            priority: &priority,
            rng: &mut *self.rng,
        };
        let r = f(&mut *self.workforce, &mut workers, &mut search);
        self.set_workers(&workers);
        r
    }

    /// A storage's drop point nearest `from` (`FUN_140132aa0` through the storage search's
    /// geometry).
    pub(crate) fn storage_point(&mut self, entity: usize, from: i32) -> Option<[f32; 3]> {
        let town = TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            floors: &self.cells.floors,
            entities: self.entities,
            happiness: self.happiness,
        };
        let (size, terrain) = (self.size, self.terrain);
        let floors = &self.cells.floors;
        let ground = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
        let mut geometry = StorageGeometry {
            town: &town,
            sectors: self.sectors,
            ground: &ground,
        };
        crate::work::stock::StorageSearchMap::storage_position(&mut geometry, entity, from)
            .map(|(_, p)| p)
    }

    /// `FUN_1400a4ce0(map, a, b, 1)`: −1 when not connected.
    pub(super) fn rank_raw(&mut self, a: i32, b: i32) -> i32 {
        let reachable = self.reachable(a, b);
        self.sectors.rebuild(&self.cells.types);
        self.sectors
            .distance(a, b, reachable, true, &self.cells.types)
    }

    /// Path rank with the callers' fallback `n + (int)sqrtf(dx² + dy²)`.
    pub(crate) fn rank_or_distance(&mut self, a: i32, b: i32) -> i32 {
        let d = self.rank_raw(a, b);
        if d >= 0 {
            return d;
        }
        let n = self.size as i32;
        let (dx, dy) = ((a % n - b % n) as f32, (a / n - b / n) as f32);
        n + (dx * dx + dy * dy).sqrt() as i32
    }

    /// An entity's model-space point in the world (`ModelComponent` transform).
    pub(crate) fn world_point(&self, entity: usize, local: [f32; 3]) -> [f32; 3] {
        let p = &self.placed[entity];
        p.model(&self.templates[p.template]).apply(local)
    }

    /// The entity, then its MultiObject parts: whose InteractComponents hold its points (the
    /// children `FUN_140128540` and `FUN_1401282f0` count on).
    pub(crate) fn interact_entities(&self, entity: usize) -> Vec<usize> {
        let mut out = vec![entity];
        if let Some(site) = self.sites.get(&entity) {
            out.extend(site.parts.iter().copied());
        }
        out
    }

    /// `FUN_1401282f0(interact, 1, i)`: build point `i` in the world (the entity's own, then
    /// its parts').
    pub(super) fn build_point(&self, entity: usize, i: usize) -> [f32; 3] {
        let mut i = i;
        for x in self.interact_entities(entity) {
            let points = &self.templates[self.placed[x].template].build_points;
            if i < points.len() {
                return self.world_point(x, points[i]);
            }
            i -= points.len();
        }
        self.world_point(entity, [0.0; 3])
    }

    pub(crate) fn build_point_count(&self, entity: usize) -> usize {
        self.interact_entities(entity)
            .iter()
            .map(|&x| self.templates[self.placed[x].template].build_points.len())
            .sum()
    }

    /// The point `FUN_1400c7e70` faces: the model's position, else the map-rectangle centre.
    pub(crate) fn facing_centre(&self, entity: usize) -> [f32; 3] {
        let p = &self.placed[entity];
        if self.templates[p.template].model.is_some() {
            p.position
        } else {
            let n = self.size as i32;
            let (x, y) = p.cell;
            let (w, h) = p.footprint;
            self.ground_at((y + (h >> 1)) * n + x + (w >> 1))
        }
    }

    /// `Work +0x28`: the owner's map-rectangle centre cell.
    pub(crate) fn rect_cell(&self, entity: usize) -> i32 {
        let p = &self.placed[entity];
        let n = self.size as i32;
        (p.cell.1 + (p.footprint.1 >> 1)) * n + (p.footprint.0 >> 1) + p.cell.0
    }
}
