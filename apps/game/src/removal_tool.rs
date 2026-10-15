//! The removal tools (`RemovalTool : DragTool`, vtable `0x140168200`): ClearTree, ClearStone,
//! ClearIron, ClearResources, ClearRoad, ClearBuildings, ClearCancel and ClearPriority, started
//! by toolbar kind 1 for a template with a `CreateRemovalDescription`. See docs/JOBS.md.
//!
//! - start (`+0x20`, `FUN_14010a0a0`): keep the template's CreateRemovalDescription (`+0x38`);
//! - input (`+0x30`, DragTool `FUN_140101b80`): MouseLeft starts a drag at the cell under the
//!   cursor; releasing on another cell applies it, releasing on the same cell waits for a second
//!   click; a new end cell plays the move sound;
//! - cancel (`+0x48`, `FUN_140101b10`): a drag in progress stops;
//! - apply (`+0x68`, `FUN_140108ab0`): every cell of the rectangle, in rows;
//! - draw (`+0x40`, `FUN_140109520`): each cell of the drag (or the hovered cell), marked when
//!   the drag would do something there.
//!
//! The original runs the tool between simulation iterations; the port does too (markers are
//! activated before the next iteration, like placed buildings).

use banished_formats::game::{
    create_drag_description, create_removal_description, toolbar_description,
    CreateDragDescription, CreateRemovalDescription, ToolbarDescription,
};
use banished_formats::pkg::Archive;
use banished_game::placement::cell_admits;
use banished_game::terrain::HeightGrid;

use crate::start::StartingTown;

fn read(archive: &Archive, reference: &str) -> Option<Vec<u8>> {
    archive.read_ref(&reference.replace('\\', "/")).ok()
}

fn same_reference(a: &str, b: &str) -> bool {
    a.replace('/', "\\")
        .eq_ignore_ascii_case(&b.replace('/', "\\"))
}

/// What a press, move or release did (the caller plays the sound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSound {
    /// `FUN_1401050c0`.
    Move,
    /// `FUN_1401055c0`.
    Thunk,
    /// `FUN_140105610`.
    Error,
}

pub struct RemovalTool {
    /// `Template\ClearX.rsc:resource`.
    pub reference: String,
    desc: CreateRemovalDescription,
    /// The marker template (`desc +0x38`) and its CreateDragDescription.
    marker: Option<usize>,
    drag: Option<CreateDragDescription>,
    pub toolbar: Option<ToolbarDescription>,
    /// DragTool `+0x28`: dragging; `+0x29`: released on the start cell, waiting for a click.
    dragging: bool,
    waiting: bool,
    /// `+0x2c` start cell, `+0x30` end cell, `+0x34` the end cell the move sound last played
    /// for (−1).
    start: i32,
    end: i32,
    sounded: i32,
    /// MouseLeft is held.
    held: bool,
}

impl RemovalTool {
    /// `FUN_14010a0a0` (and the constructor `FUN_14010a190`).
    pub fn start(archive: &Archive, town: &mut StartingTown, reference: &str) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let desc = create_removal_description(&read(archive, &format!("{file}:createremoval"))?)
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()?;
        let marker = desc
            .template
            .as_deref()
            .and_then(|t| town.load_template(archive, t));
        let drag = desc.template.as_deref().and_then(|t| {
            let file = t.split(':').next()?.replace('\\', "/");
            create_drag_description(&read(archive, &format!("{file}:createdrag"))?).ok()
        });
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        Some(RemovalTool {
            reference: reference.to_string(),
            desc,
            marker,
            drag,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
        })
    }

    /// The drag's start and end cells while dragging (for the CreateBar).
    pub fn drag_ends(&self) -> Option<(i32, i32)> {
        (self.dragging && self.start >= 0 && self.end >= 0).then_some((self.start, self.end))
    }

    /// Slot `+0x70` (`FUN_1401093c0`): the drag's width and height in cells.
    pub fn drag_size(&self, n: i32) -> Option<(i32, i32)> {
        let (a, b) = self.drag_ends()?;
        let (x0, y0, x1, y1) = self.rect(n, a, b);
        Some((x1 - x0 + 1, y1 - y0 + 1))
    }

    /// Apply a drag from cell `start` to cell `end` (scripted runs).
    pub fn drag(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        start: i32,
        end: i32,
    ) -> DragSound {
        self.start = start;
        self.end = end;
        self.apply(archive, town, terrain)
    }

    /// MouseLeft pressed with the cursor over `cell` (−1 off the map).
    pub fn press(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        cell: i32,
    ) -> Option<DragSound> {
        self.held = true;
        if !self.dragging {
            self.start = cell;
            self.end = cell;
            if cell >= 0 {
                self.dragging = true;
                self.waiting = false;
                return self.track(cell);
            }
            return None;
        }
        if self.waiting {
            // The second click applies.
            let sound = if cell < 0 {
                DragSound::Error
            } else {
                self.end = cell;
                self.apply(archive, town, terrain)
            };
            self.dragging = false;
            self.waiting = false;
            return Some(sound);
        }
        None
    }

    /// The cursor moved over `cell`; while MouseLeft is up (released off the map) the drag
    /// completes as soon as the cursor is back on it.
    pub fn motion(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        cell: i32,
    ) -> Option<DragSound> {
        if !self.dragging {
            return None;
        }
        let sound = if cell >= 0 { self.track(cell) } else { None };
        if !self.waiting && !self.held {
            return self.finish(archive, town, terrain, cell).or(sound);
        }
        sound
    }

    /// MouseLeft released over `cell`.
    pub fn release(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        cell: i32,
    ) -> Option<DragSound> {
        self.held = false;
        if !self.dragging || self.waiting {
            return None;
        }
        if cell >= 0 {
            self.track(cell);
        }
        self.finish(archive, town, terrain, cell)
    }

    /// `FUN_140101b10`: stop a drag in progress; false when there was none.
    pub fn cancel(&mut self) -> bool {
        if !self.dragging {
            return false;
        }
        self.dragging = false;
        self.waiting = false;
        true
    }

    /// The end cell follows the cursor; a new one plays the move sound.
    fn track(&mut self, cell: i32) -> Option<DragSound> {
        self.end = cell;
        if self.sounded != self.end {
            self.sounded = self.end;
            return Some(DragSound::Move);
        }
        None
    }

    /// The button is up while dragging: on the start cell wait for a click, off the map
    /// complain, else apply.
    fn finish(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        cell: i32,
    ) -> Option<DragSound> {
        if self.start == self.end {
            self.waiting = true;
            return None;
        }
        if cell < 0 {
            return Some(DragSound::Error);
        }
        let sound = self.apply(archive, town, terrain);
        self.dragging = false;
        Some(sound)
    }

    fn rect(&self, n: i32, a: i32, b: i32) -> (i32, i32, i32, i32) {
        let (ax, ay, bx, by) = (a % n, a / n, b % n, b / n);
        (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by))
    }

    /// The entity a cell answers for: the occupant (`+0x00`), else the object (`+0x08`).
    fn entity_at(town: &StartingTown, i: usize) -> Option<usize> {
        town.cells.occupants[i]
            .or(town.cells.objects[i])
            .filter(|&e| town.placed[e].alive)
    }

    /// `FUN_140109d70`: mark this natural resource for removal.
    fn marks(
        &self,
        town: &StartingTown,
        terrain: &HeightGrid,
        e: Option<usize>,
        x: i32,
        y: i32,
    ) -> bool {
        let (Some(drag), Some(e)) = (&self.drag, e) else {
            return false;
        };
        if !self.desc.resources || self.marker.is_none() {
            return false;
        }
        let t = &town.templates[town.placed[e].template];
        if t.natural.is_none() {
            return false;
        }
        if !self.desc.filters.is_empty()
            && !self
                .desc
                .filters
                .iter()
                .any(|f| same_reference(f, &t.reference))
        {
            return false;
        }
        // `FUN_1400a4aa0(map, x, y, drag +0x20, 0, 100, 0, drag +0x24, false, null)`.
        cell_admits(
            &town.cells,
            terrain,
            x,
            y,
            drag.mask,
            0.0,
            100.0,
            0.0,
            drag.override_occupant,
            false,
            |_| false,
        )
    }

    /// `FUN_140109c40`: a removal marker to take away, or a building being torn down
    /// (DestroyComponent `FUN_140123be0`).
    fn cancels(&self, town: &StartingTown, e: Option<usize>) -> bool {
        let Some(e) = e else { return false };
        if !self.desc.cancel {
            return false;
        }
        Some(town.placed[e].template) == self.marker
            || (Self::has_destroy(town, e) && matches!(Self::shown_state(town, e), 3..=5))
    }

    fn has_destroy(town: &StartingTown, e: usize) -> bool {
        town.construction
            .sites
            .get(&e)
            .is_some_and(|s| s.destroy.is_some())
    }

    /// The StateMachine's pending state, else its state (`+0x52`, `+0x50`).
    fn shown_state(town: &StartingTown, e: usize) -> i8 {
        town.construction.sites.get(&e).map_or(-1, |s| {
            if s.machine.pending < 0 {
                s.machine.state
            } else {
                s.machine.pending
            }
        })
    }

    /// `FUN_140109b40`: a building (not the marker) with a DestroyComponent not already being
    /// torn down.
    fn demolishes(&self, town: &StartingTown, e: Option<usize>) -> bool {
        let Some(e) = e else { return false };
        self.desc.buildings
            && Some(town.placed[e].template) != self.marker
            && Self::has_destroy(town, e)
            && !matches!(Self::shown_state(town, e), 3..=5)
    }

    fn command(town: &mut StartingTown, command: banished_game::professions::Command) {
        if !town.construction.commands.contains(&command) {
            town.construction.commands.push(command);
        }
    }

    /// `FUN_140133fa0` on the entity's WorkComponent: its work is registered.
    fn work_of(town: &StartingTown, e: Option<usize>) -> Option<i32> {
        let site = town.construction.sites.get(&e?)?;
        town.construction
            .jobs
            .works
            .contains_key(&site.work_id)
            .then_some(site.work_id)
    }

    /// `FUN_140108ab0` over the dragged rectangle: markers, cancels, demolition
    /// (`FUN_140109b40` → `FUN_140123470`), priority. Not ported: ClearRoad (`FUN_1400a1bd0`,
    /// roads).
    pub fn apply(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
    ) -> DragSound {
        let n = town.cells.size as i32;
        let (x0, y0, x1, y1) = self.rect(n, self.start, self.end);
        let before = town.placed.len();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = (y * n + x) as usize;
                let e = Self::entity_at(town, i);
                if self.marks(town, terrain, e, x, y) {
                    if let Some(t) = self.marker {
                        town.add_marker(archive, t, (x, y), terrain);
                    }
                }
                if self.cancels(town, e) {
                    let e = e.unwrap();
                    if Some(town.placed[e].template) == self.marker {
                        if !town.construction.destroy.contains(&e) {
                            town.construction.destroy.push(e);
                        }
                    } else {
                        // The state machine's message 0: the demolition is called off.
                        Self::command(
                            town,
                            banished_game::professions::Command::StateMessage {
                                entity: e,
                                message: 0,
                            },
                        );
                    }
                }
                if self.demolishes(town, e) {
                    let entity = e.unwrap();
                    Self::command(
                        town,
                        banished_game::professions::Command::Demolish { entity },
                    );
                }
                if self.desc.priority {
                    if let Some(id) = Self::work_of(town, e) {
                        town.construction.jobs.prioritize(id);
                    }
                }
            }
        }
        log::info!(
            "{}: {}×{} cells, {} markers",
            self.reference,
            x1 - x0 + 1,
            y1 - y0 + 1,
            town.placed.len() - before
        );
        DragSound::Thunk
    }

    /// `FUN_140109520`: the cells to draw, each with whether the drag would act on it.
    pub fn cells(
        &self,
        town: &StartingTown,
        terrain: &HeightGrid,
        hover: i32,
    ) -> Vec<(i32, i32, bool)> {
        let (a, b) = if self.dragging {
            (self.start, self.end)
        } else {
            (hover, hover)
        };
        if a < 0 {
            return Vec::new();
        }
        let b = if b < 0 { a } else { b };
        let n = town.cells.size as i32;
        let (x0, y0, x1, y1) = self.rect(n, a, b);
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let e = Self::entity_at(town, (y * n + x) as usize);
                let acts = self.marks(town, terrain, e, x, y)
                    || self.cancels(town, e)
                    || self.demolishes(town, e)
                    || (self.desc.priority && Self::work_of(town, e).is_some());
                out.push((x, y, acts));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use banished_formats::pkg::Package;
    use banished_game::random::Random;

    /// ClearTree over the trees near the start: one marker per tree cell, laborers fell them,
    /// every marker removes itself, and ClearCancel takes markers away again.
    #[test]
    fn clear_tree_marks_and_fells() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::world::World::generate(
            &archive,
            &crate::world::WorldParams {
                seed: 1234,
                ..Default::default()
            },
        )
        .unwrap();
        let terrain = HeightGrid {
            width: world.terrain.width(),
            heights: &world.terrain.heights,
        };
        let mut rng = Random::new(1234);
        let mut town = StartingTown::create(
            &archive,
            "Game/StartCondition/Medium.rsc:resource",
            world.terrain.size,
            &terrain,
            &mut rng,
        )
        .unwrap();
        let n = town.cells.size as i32;
        let (x, y) = town.start;
        let r = 12;
        let (a, b) = ((y - r) * n + x - r, (y + r) * n + x + r);
        let is_tree = |town: &StartingTown, i: usize| {
            town.cells.objects[i].is_some_and(|o| {
                town.placed[o].alive
                    && town.templates[town.placed[o].template]
                        .reference
                        .contains("NaturalResourceTree")
            })
        };
        let cells: Vec<usize> = (y - r..=y + r)
            .flat_map(|cy| (x - r..=x + r).map(move |cx| (cy * n + cx) as usize))
            .collect();
        let trees = cells.iter().filter(|&&i| is_tree(&town, i)).count();
        let mut tool =
            RemovalTool::start(&archive, &mut town, r"Template\ClearTree.rsc:resource").unwrap();
        let before = town.placed.len();
        tool.drag(&archive, &mut town, &terrain, a, b);
        let markers: Vec<usize> = (before..town.placed.len()).collect();
        assert!(!markers.is_empty(), "no trees near the start");
        assert!(markers.len() <= trees);
        // A second drag marks nothing new: every marked cell has an occupant.
        tool.drag(&archive, &mut town, &terrain, a, b);
        assert_eq!(town.placed.len(), before + markers.len());

        // Cancel one marker before anyone works on it.
        let mut cancel =
            RemovalTool::start(&archive, &mut town, r"Template\ClearCancel.rsc:resource").unwrap();
        let first = markers[0];
        let (cx, cy) = town.placed[first].cell;
        cancel.drag(&archive, &mut town, &terrain, cy * n + cx, cy * n + cx);
        assert_eq!(town.construction.destroy, [first]);

        let mut warnings = banished_game::events::StockWarnings::default();
        let dt = 1.0 / 30.0;
        let mut done_at = None;
        for step in 0..30 * 60 * 10 {
            town.apply_commands(&terrain, &mut rng);
            town.iterate(&terrain, &mut rng, dt, 50.0);
            let stock = std::array::from_fn(|i| (warnings.count[i], warnings.limits[i]));
            town.update_citizens(&terrain, &mut rng, dt, 50.0, stock);
            warnings.recompute(&town.registered_storages(), &town.materials);
            town.construction
                .jobs
                .step(banished_game::scheduler::ticks(dt));
            if step == 0 {
                assert!(!town.placed[first].alive, "cancelled marker still alive");
                assert_eq!(town.cells.occupants[(cy * n + cx) as usize], None);
            }
            if markers.iter().all(|&m| !town.placed[m].alive) {
                done_at = Some(step);
                break;
            }
        }
        let left = markers.iter().filter(|&&m| town.placed[m].alive).count();
        eprintln!(
            "{} markers, {left} left, done at {done_at:?}",
            markers.len()
        );
        assert!(done_at.is_some(), "markers left: {left}");
        for &m in &markers[1..] {
            let (mx, my) = town.placed[m].cell;
            let i = (my * n + mx) as usize;
            assert!(!is_tree(&town, i), "tree at {mx},{my} not felled");
            assert_eq!(town.cells.occupants[i], None);
        }
        let (fx, fy) = town.placed[first].cell;
        assert!(
            is_tree(&town, (fy * n + fx) as usize),
            "cancelled tree was felled"
        );
        assert!(town.construction.sites.keys().all(|e| !markers.contains(e)));
    }
}
