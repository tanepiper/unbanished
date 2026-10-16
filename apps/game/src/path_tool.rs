//! The Path tool (`PathTool`, vtable `0x140166aa0`), started by the toolbar's Path entry
//! (`Template\UtilityPath.rsc`, its `CreatePathDescription`; its cursor is 4, Query,
//! `FUN_1400e3de0`).
//!
//! - **Input** (`+0x30`, `FUN_1400e3b80`): a left click picks what is under the cursor (the
//!   map's object at the cell, else the picking scene's hit) and, when it is another entity,
//!   shows its paths (`FUN_1400e2be0`): the old entity's highlight goes (`FUN_1400e2720`), the
//!   new one and its parts are highlighted (`FUN_1400e25c0`), then `FUN_1400e2c40`.
//! - **Paths** (`FUN_1400e2c40`): the commute of a citizen with both a home (`+0x238`) and a
//!   workplace (`+0x248`): from the home's point 0 (`FUN_1400e3aa0`) to the workplace's
//!   entrance (`FUN_14008d480`, which draws `Rand(points)` from the game's Random when there
//!   are several). The picked citizen's, then a workplace's workers (WorkPlaceComponent with a
//!   profession, `+0x58`), else a residence's families slot by slot (`+0x50`). Each is the
//!   path manager's walk (`FUN_1400a2480`, mask `0xe1f`), at most `0x800` cells; the tool keeps
//!   the set of cells (`+0x40`) and of steps, each as its (lower, higher) cell pair (`+0x50`).
//! - **Draw** (`+0x40`, `FUN_1400e2880`): every step a line (frame 5, half width 0.15) and
//!   every cell a quad (frame 4), coloured `0xfffccc4c` (`FUN_140105d90`).
//! - **End** (`+0x28`, `FUN_1400e2720`): the highlight goes.

use std::collections::BTreeSet;

use banished_game::random::Random;

use crate::selection::Entity;
use crate::selection_overlay::{OverlayCell, OverlayLine};
use crate::start::StartingTown;

/// `FUN_1400e2c40`: the longest path kept.
const MAX_CELLS: usize = 0x800;
/// `FUN_1400e2880`: the overlay's colour, frames and line half width.
const COLOUR: u32 = 0xfffc_cc4c;
const CELL_FRAME: i32 = 4;
const LINE_FRAME: i32 = 5;
const LINE_WIDTH: f32 = 0.15;

#[derive(Default)]
pub struct PathTool {
    /// `+0x28`: the picked entity.
    pub entity: Option<Entity>,
    /// `+0x40`, `+0x50`.
    cells: BTreeSet<i32>,
    steps: BTreeSet<(i32, i32)>,
}

impl PathTool {
    /// `FUN_140108030` for a template with a `CreatePathDescription`.
    pub fn start(archive: &banished_formats::pkg::Archive, template: &str) -> Option<PathTool> {
        let file = template.split(':').next()?.replace('\\', "/");
        archive.read_ref(&format!("{file}:createpath")).ok()?;
        Some(PathTool::default())
    }

    /// `FUN_1400e2be0`: show `entity`'s paths unless it is already picked.
    pub fn pick(&mut self, town: &mut StartingTown, rng: &mut Random, entity: Entity) {
        if self.entity == Some(entity) {
            return;
        }
        self.entity = Some(entity);
        self.find_paths(town, rng);
    }

    /// `FUN_1400e2c40`.
    fn find_paths(&mut self, town: &mut StartingTown, rng: &mut Random) {
        self.cells.clear();
        self.steps.clear();
        let Some(entity) = self.entity else { return };
        let mut citizens = Vec::new();
        if let Entity::Citizen(c) = entity {
            citizens.push(c);
        }
        if let Entity::Placed(e) = entity {
            let c = &town.construction;
            match c.workforce.workplaces.get(&e) {
                Some(w) if w.profession.is_some() => citizens.extend(&w.workers),
                _ => {
                    if let Some(home) = c.homes.get(&e) {
                        citizens.extend(home.families.iter().flatten());
                    }
                }
            }
        }
        let views =
            banished_game::agents::entity_views(&town.placed, &town.templates, &town.construction);
        let view = banished_game::activities::TownView {
            size: town.cells.size,
            types: &town.cells.types,
            regions: &[],
            occupants: &town.cells.occupants,
            objects: &town.cells.objects,
            floors: &town.cells.floors,
            entities: &views,
            happiness: &[],
        };
        let mut requests = Vec::new();
        for c in citizens {
            let Some(s) = town.agents.states.get(c) else {
                continue;
            };
            let (Some(home), Some(work)) = (s.home, s.workplace) else {
                continue;
            };
            let to = view.entrance(work, rng);
            requests.push((view.point_cell(home), to));
        }
        for (from, to) in requests {
            if from < 0 || to < 0 {
                continue;
            }
            let Some(mut path) = town.agents.walk_path(&town.cells, from, to) else {
                continue;
            };
            path.truncate(MAX_CELLS);
            for (i, &cell) in path.iter().enumerate() {
                self.cells.insert(cell);
                if i > 0 {
                    let prev = path[i - 1];
                    self.steps.insert((prev.min(cell), prev.max(cell)));
                }
            }
        }
    }

    /// `FUN_1400e2880`: the lines, then the cells.
    pub fn overlay(&self, map_size: i32) -> (Vec<OverlayLine>, Vec<OverlayCell>) {
        let at = |c: i32| (c % map_size, c / map_size);
        let lines = self
            .steps
            .iter()
            .map(|&(a, b)| OverlayLine {
                a: at(a),
                b: at(b),
                frame: LINE_FRAME,
                argb: COLOUR,
                width: LINE_WIDTH,
            })
            .collect();
        let cells = self
            .cells
            .iter()
            .map(|&c| OverlayCell {
                x: c % map_size,
                y: c / map_size,
                frame: CELL_FRAME,
                argb: COLOUR,
                rotation: 0,
            })
            .collect();
        (lines, cells)
    }
}
