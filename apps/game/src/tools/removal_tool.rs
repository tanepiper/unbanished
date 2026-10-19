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
//!
//! RoadTool (`RoadTool : DragTool`, vtable `0x140168290`, for a template with a
//! `CreateRoadDescription`) shares the DragTool input and runs here as the road mode: start
//! `FUN_14010abb0`, apply `FUN_14010a200`, check `FUN_14010a4b0` (see docs/ROADS.md).

use banished_formats::game::{
    create_drag_description, create_removal_description, create_road_description,
    toolbar_description, CreateDragDescription, CreateRemovalDescription, ToolbarDescription,
};
use banished_formats::pkg::Archive;
use banished_game::terrain::HeightGrid;
use banished_game::world::placement::cell_admits;

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
    /// The road mode (RoadTool `+0x38`): the CreateRoadDescription's kind and material, and
    /// the cell type of the road template's MapDescription.
    road: Option<RoadSpec>,
    /// The bridge mode (BridgeTool `+0x38`): the CreateBridgeDescription and its parts.
    bridge: Option<crate::start::BridgeSpec>,
    /// The area mode (DragPlacementTool `+0x38`, `+0x40`): the template, its
    /// CreateDragDescription and the template whose entities may stand in the way.
    area: Option<AreaSpec>,
    /// The fenced mode (FencedTool): the CreateFencedDescription and its clear template.
    fenced: Option<(
        banished_formats::game::CreateFencedDescription,
        Option<usize>,
    )>,
    /// The RotationSetting (`+0x50 → +0x10`) the fenced tool turns its shelter by.
    pub rotation: i32,
    /// A RestrictPlayer setup's cells (game UI `+0x80`, corners `+0x98`/`+0x9c` as `(x0, y0,
    /// x1, y1)`): the drag must cover exactly them (a road must run between those corners).
    pub restrict: Option<[i32; 4]>,
    /// RoadTool `+0x40` (`FUN_14010ab60`): the path's Manhattan heuristic, on while
    /// PathMethod (action `0x16`) is not held.
    pub manhattan: bool,
}

#[derive(Debug, Clone)]
struct AreaSpec {
    template: usize,
    drag: CreateDragDescription,
    clear: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct RoadSpec {
    kind: u8,
    ty: u8,
    material: Option<usize>,
}

/// `0xa17`: the cell types a road crosses (0–2, 4, 9, 11); `0x5e0` the types its ends may not
/// be (water, high and rough ground).
const ROAD_MASK: u32 = 0xa17;
const ROAD_ENDS: u32 = 0x5e0;

impl RemovalTool {
    /// `FUN_14010a0a0` (and the constructor `FUN_14010a190`).
    pub fn start(archive: &Archive, town: &mut StartingTown, reference: &str) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let Some(data) = read(archive, &format!("{file}:createremoval")) else {
            return Self::start_road(archive, town, reference);
        };
        let desc = create_removal_description(&data)
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
        // Remove Roads' markers are placed by the town after the simulation decides them.
        if desc.roads {
            if let Some(t) = marker {
                town.site_template(archive, t);
            }
        }
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
            road: None,
            bridge: None,
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        })
    }

    /// RoadTool's start (`FUN_14010abb0`): the template's CreateRoadDescription.
    fn start_road(archive: &Archive, town: &mut StartingTown, reference: &str) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let Some(data) = read(archive, &format!("{file}:createroad")) else {
            return Self::start_bridge(archive, town, reference);
        };
        let d = create_road_description(&data)
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()?;
        let template = town.load_template(archive, reference)?;
        let ty = town.templates[template]
            .map
            .as_ref()
            .map_or(0, |m| m.cell_type as u8);
        let material = d
            .material
            .as_deref()
            .and_then(|m| crate::start::material_index(archive, &mut town.materials, m));
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        Some(RemovalTool {
            reference: reference.to_string(),
            desc: CreateRemovalDescription::default(),
            marker: None,
            drag: None,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: Some(RoadSpec {
                kind: d.kind,
                ty,
                material,
            }),
            bridge: None,
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        })
    }

    /// BridgeTool's start (`FUN_140100c70`): the template's CreateBridgeDescription.
    fn start_bridge(archive: &Archive, town: &mut StartingTown, reference: &str) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let Some(bridge) = town.bridge_spec(archive, reference) else {
            return Self::start_area(archive, town, reference);
        };
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        Some(RemovalTool {
            reference: reference.to_string(),
            desc: CreateRemovalDescription::default(),
            marker: None,
            drag: None,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: None,
            bridge: Some(bridge),
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        })
    }

    /// DragPlacementTool's start (`FUN_1401018b0`): the template's CreateDragDescription and
    /// MapDescription; or FencedTool's (`FUN_1400e2220`) for a CreateFencedDescription.
    fn start_area(archive: &Archive, town: &mut StartingTown, reference: &str) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        let template = town.load_template(archive, reference)?;
        let mut tool = RemovalTool {
            reference: reference.to_string(),
            desc: CreateRemovalDescription::default(),
            marker: None,
            drag: None,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: None,
            bridge: None,
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        };
        if let Some(data) = read(archive, &format!("{file}:createdrag")) {
            let drag = create_drag_description(&data)
                .map_err(|e| log::warn!("{reference}: {e}"))
                .ok()?;
            let clear = drag
                .reference
                .as_deref()
                .and_then(|c| town.load_template(archive, c));
            tool.area = Some(AreaSpec {
                template,
                drag,
                clear,
            });
            return Some(tool);
        }
        let d = banished_formats::game::create_fenced_description(&read(
            archive,
            &format!("{file}:createfenced"),
        )?)
        .map_err(|e| log::warn!("{reference}: {e}"))
        .ok()?;
        let clear = d
            .clear
            .as_deref()
            .and_then(|c| town.load_template(archive, c));
        tool.fenced = Some((d, clear));
        Some(tool)
    }

    /// DragPlacementTool's check (slot `+0x70`, `FUN_140100fb0`) of the rectangle from `a` to
    /// `b`: 1 smaller than `+0x30 × +0x34`, 2 larger than `+0x28 × +0x2c`, 3 a cell that does
    /// not admit it (`FUN_1400a4aa0(mask +0x20, 0, 100, 0, +0x24, the clear template +0x38)`),
    /// else 0.
    fn area_check(
        town: &StartingTown,
        spec: &AreaSpec,
        terrain: &HeightGrid,
        a: i32,
        b: i32,
    ) -> u8 {
        let n = town.cells.size as i32;
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let d = &spec.drag;
        if w < d.value_30 || h < d.value_34 {
            return 1;
        }
        if d.value_28 < w || d.value_2c < h {
            return 2;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                if !Self::area_cell(town, spec, terrain, x, y) {
                    return 3;
                }
            }
        }
        0
    }

    fn area_cell(
        town: &StartingTown,
        spec: &AreaSpec,
        terrain: &HeightGrid,
        x: i32,
        y: i32,
    ) -> bool {
        cell_admits(
            &town.cells,
            terrain,
            x,
            y,
            spec.drag.mask,
            0.0,
            100.0,
            0.0,
            spec.drag.override_occupant,
            false,
            |o| Some(town.placed[o].template) == spec.clear,
        )
    }

    /// `FUN_140100290`: the bridge's cells from the drag's start to its end on the same row or
    /// column, valid (green) where the check holds and the cell is of a spanned or shown type
    /// with no one on it, or, before the drag has length, a bank.
    fn bridge_cells(
        town: &StartingTown,
        spec: &crate::start::BridgeSpec,
        a: i32,
        b: i32,
    ) -> Vec<(i32, i32, bool)> {
        let n = town.cells.size as i32;
        let b = if b < 0 { a } else { town.bridge_end(a, b) };
        let (ok, _) = town.bridge_check(spec, a, b);
        let d = &spec.desc;
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = (y * n + x) as usize;
                let t = 1u32 << (town.cells.types[i].0 & 0x1f);
                let valid =
                    (ok && (d.span | d.shown) & t != 0 && town.cells.occupants[i].is_none())
                        || (a == b && d.banks & t != 0);
                out.push((x, y, valid));
            }
        }
        out
    }

    /// `FUN_14010a4b0` / the start of `FUN_14010a200`: a road's cells between the drag's ends
    /// (sorted), the ends neither water, high nor rough ground nor occupied. The path manager
    /// steps without diagonals, with the Manhattan heuristic unless key 0x16 is held
    /// (`FUN_14010ab60`).
    /// The check and the draw (`FUN_14010a4b0`, `FUN_14010a660`) let the path cross the ends'
    /// own types too (`ends | 0xa17`); the apply (`FUN_14010a200`) only `0xa17`.
    fn road_cells(&self, town: &mut StartingTown, a: i32, b: i32) -> Option<Vec<i32>> {
        self.road_path(town, a, b, true)
    }

    fn road_path(
        &self,
        town: &mut StartingTown,
        a: i32,
        b: i32,
        with_ends: bool,
    ) -> Option<Vec<i32>> {
        let (a, b) = if b < a { (b, a) } else { (a, b) };
        if a < 0 {
            return None;
        }
        let (ia, ib) = (a as usize, b as usize);
        let ends =
            (1u32 << (town.cells.types[ia].0 & 0x1f)) | (1u32 << (town.cells.types[ib].0 & 0x1f));
        if ends & ROAD_ENDS != 0
            || town.cells.occupants[ia].is_some()
            || town.cells.occupants[ib].is_some()
        {
            return None;
        }
        let mask = if with_ends {
            ends | ROAD_MASK
        } else {
            ROAD_MASK
        };
        town.agents
            .road_path(&town.cells, a, b, mask, self.manhattan)
            .filter(|p| !p.is_empty())
    }

    /// The drag's start and end cells while dragging (for the CreateBar).
    pub fn drag_ends(&self) -> Option<(i32, i32)> {
        (self.dragging && self.start >= 0 && self.end >= 0).then_some((self.start, self.end))
    }

    /// Off a RestrictPlayer setup's cells (`FUN_1401093c0`, `FUN_140108ab0`, `FUN_140100fb0`,
    /// `FUN_1400e0960`, `FUN_14010a4b0`, `FUN_14010a200`: message 3, the apply fails). Bridges
    /// are not held to it.
    fn off_restrict(&self, n: i32, a: i32, b: i32) -> bool {
        let Some([x0, y0, x1, y1]) = self.restrict else {
            return false;
        };
        if self.bridge.is_some() {
            return false;
        }
        if self.road.is_some() {
            let (c0, c1) = (y0 * n + x0, y1 * n + x1);
            let (a, b) = (a.min(b), a.max(b));
            return !((a == c0 && b == c1) || (a == c1 && b == c0));
        }
        let rect = (
            (a % n).min(b % n),
            (a / n).min(b / n),
            (a % n).max(b % n),
            (a / n).max(b / n),
        );
        rect != (x0, y0, x1, y1)
    }

    /// Slot `+0x70` for the CreateBar while dragging (`FUN_140101b80`):
    /// - RemovalTool (`FUN_1401093c0`) and DragPlacementTool (`FUN_140100fb0`, its check): the
    ///   rectangle `W×H`, the requirements once;
    /// - FencedTool (`FUN_1400e0960`): the rectangle, the requirements once per fence cell
    ///   (`2(W + H) − 4`); 1 too small, 2 too large, 3 a cell that will not take it;
    /// - RoadTool (`FUN_14010a4b0`): the road's length, the requirements per cell; without a
    ///   road no size and message 1;
    /// - BridgeTool (`FUN_1401001c0`): the span's length, the requirements per cell; else 3.
    pub fn drag_info(
        &self,
        town: &mut StartingTown,
        terrain: &HeightGrid,
    ) -> Option<crate::ui::create_bar::DragInfo> {
        use crate::ui::create_bar::DragInfo;
        let (a, b) = self.drag_ends()?;
        let n = town.cells.size as i32;
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let restricted = self.off_restrict(n, a, b);
        let rect = |count, message| DragInfo {
            mode: 0,
            width: w,
            height: h,
            count,
            message,
        };
        let length = |length, message| DragInfo {
            mode: 1,
            width: length,
            height: length,
            count: length,
            message,
        };
        if let Some(spec) = &self.bridge {
            let end = town.bridge_end(a, b);
            return Some(match town.bridge_check(spec, a, end) {
                (true, span) => length(span, 0),
                _ => length(0, 3),
            });
        }
        if let Some(spec) = &self.area {
            let message = Self::area_check(town, spec, terrain, a, b);
            return Some(rect(
                1,
                if message == 0 && restricted {
                    3
                } else {
                    message.into()
                },
            ));
        }
        if let Some((d, clear)) = &self.fenced {
            let message = if w < d.min.0 || h < d.min.1 {
                1
            } else if d.max.0 < w || d.max.1 < h {
                2
            } else if restricted
                || (y0..=y1)
                    .any(|y| (x0..=x1).any(|x| !town.fenced_cell_ok(d, *clear, x, y, terrain)))
            {
                3
            } else {
                0
            };
            return Some(rect((w + h) * 2 - 4, message));
        }
        if self.road.is_some() {
            if restricted {
                return Some(DragInfo {
                    mode: 2,
                    width: 0,
                    height: 0,
                    count: 1,
                    message: 3,
                });
            }
            return Some(match self.road_cells(town, a, b) {
                Some(path) => length(path.len() as i32, 0),
                None => DragInfo {
                    mode: 2,
                    width: 0,
                    height: 0,
                    count: 1,
                    message: 1,
                },
            });
        }
        Some(rect(1, if restricted { 3 } else { 0 }))
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
        town.cells.entity_at(i).filter(|&e| town.placed[e].alive)
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

    /// Remove Roads (`+0x40`): a road cell (types 0, 1), a planned one or a road plan.
    fn clears_road(&self, town: &StartingTown, i: usize) -> bool {
        if !self.desc.roads {
            return false;
        }
        let t = town.cells.types[i].0;
        t == 0
            || t == 1
            || town.cells.road_kind[i] != banished_game::work::road::NONE
            || town.construction.pending_roads.contains_key(&(i as i32))
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

    fn command(town: &mut StartingTown, command: banished_game::work::professions::Command) {
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

    /// `FUN_140108ab0` over the dragged rectangle: markers, roads (`FUN_1400a1bd0`), cancels,
    /// demolition (`FUN_140109b40` → `FUN_140123470`), priority. In the road mode
    /// (`FUN_14010a200`), every cell of the road is planned (`FUN_1400a1e30`); the road decals
    /// follow the road revision (`road_decals.rs`).
    pub fn apply(
        &mut self,
        archive: &Archive,
        town: &mut StartingTown,
        terrain: &HeightGrid,
    ) -> DragSound {
        if self.off_restrict(town.cells.size as i32, self.start, self.end) {
            return DragSound::Error;
        }
        if let Some(spec) = &self.area {
            // `FUN_140100dd0`: the clear template's entities in the way go (`FUN_140106da0`);
            // the entity of the template over the rectangle, turned 90° unless wider than
            // long.
            if Self::area_check(town, spec, terrain, self.start, self.end) != 0 {
                return DragSound::Error;
            }
            let n = town.cells.size as i32;
            let (a, b) = (self.start, self.end);
            let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
            let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
            if let Some(c) = spec.clear {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if let Some(o) = town.cells.occupants[(y * n + x) as usize] {
                            if town.placed[o].template == c
                                && !town.construction.destroy.contains(&o)
                            {
                                town.construction.destroy.push(o);
                            }
                        }
                    }
                }
            }
            let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
            let reference = town.templates[spec.template].reference.clone();
            return match town.place_area(archive, &reference, (x0, y0, w, h), terrain) {
                Some(e) => {
                    town.placed[e].yaw = if h < w { 0.0 } else { 90.0 };
                    DragSound::Thunk
                }
                None => DragSound::Error,
            };
        }
        if self.fenced.is_some() {
            let n = town.cells.size as i32;
            let (a, b) = (self.start, self.end);
            return match town.place_fenced(
                archive,
                &self.reference,
                (a % n, a / n),
                (b % n, b / n),
                self.rotation,
                terrain,
            ) {
                Some(_) => DragSound::Thunk,
                None => DragSound::Error,
            };
        }
        if let Some(spec) = &self.bridge {
            return match town.place_bridge(archive, spec, self.start, self.end, terrain) {
                Some(_) => DragSound::Thunk,
                None => DragSound::Error,
            };
        }
        if let Some(r) = self.road {
            let Some(cells) = self.road_path(town, self.start, self.end, false) else {
                return DragSound::Error;
            };
            for cell in cells {
                town.construction.commands.push(
                    banished_game::work::professions::Command::PlanRoad {
                        cell,
                        kind: r.kind,
                        ty: r.ty,
                        material: r.material,
                    },
                );
            }
            return DragSound::Thunk;
        }
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
                if self.clears_road(town, i) {
                    if let Some(marker) = self.marker {
                        Self::command(
                            town,
                            banished_game::work::professions::Command::ClearRoad {
                                cell: i as i32,
                                marker,
                            },
                        );
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
                            banished_game::work::professions::Command::StateMessage {
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
                        banished_game::work::professions::Command::Demolish { entity },
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

    /// The overlay of [`Self::cells`]: the removal tools mark the cells they act on red with
    /// the cross (`FUN_140109520`); the building tools draw green where the cell takes them
    /// and red where not, the road's cells with frame 1 (`FUN_14010a660`), the bridge's
    /// spanned cells too (`FUN_140100290`), the areas' and fences' plain (`FUN_140101310`,
    /// `FUN_1400e1910`).
    pub fn overlay(
        &self,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        hover: i32,
    ) -> Vec<crate::gfx::selection_overlay::OverlayCell> {
        use crate::gfx::selection_overlay::OverlayCell;
        let n = town.cells.size as i32;
        let cells = self.cells(town, terrain, hover);
        let builds = self.road.is_some()
            || self.bridge.is_some()
            || self.area.is_some()
            || self.fenced.is_some();
        cells
            .into_iter()
            .map(|(x, y, ok)| {
                if !builds {
                    return OverlayCell::removal(x, y, ok);
                }
                let frame = if self.road.is_some() {
                    1
                } else if let Some(spec) = &self.bridge {
                    let t = 1u32 << (town.cells.types[(y * n + x) as usize].0 & 0x1f);
                    (spec.desc.span & t != 0) as i32
                } else {
                    0
                };
                OverlayCell {
                    x,
                    y,
                    frame,
                    argb: if ok { 0xff00_ff00 } else { 0xffff_0000 },
                    rotation: 0,
                }
            })
            .collect()
    }

    /// `FUN_140109520`: the cells to draw, each with whether the drag would act on it.
    pub fn cells(
        &self,
        town: &mut StartingTown,
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
        if let Some(spec) = &self.bridge {
            return Self::bridge_cells(town, spec, a, b);
        }
        if let Some(spec) = &self.area {
            // `FUN_140101310`: each cell green unless the drag is too large or the cell does
            // not admit it.
            let n = town.cells.size as i32;
            let b = if b < 0 { a } else { b };
            let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
            let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
            let large = spec.drag.value_28 < x1 - x0 + 1 || spec.drag.value_2c < y1 - y0 + 1;
            let mut out = Vec::new();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    out.push((x, y, !large && Self::area_cell(town, spec, terrain, x, y)));
                }
            }
            return out;
        }
        if let Some((d, clear)) = &self.fenced {
            // FencedTool's draw (`FUN_1400e1910`): the rectangle's cells, each by
            // `FUN_1400e0960`'s test.
            let n = town.cells.size as i32;
            let b = if b < 0 { a } else { b };
            let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
            let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
            let large = d.max.0 < x1 - x0 + 1 || d.max.1 < y1 - y0 + 1;
            let mut out = Vec::new();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    out.push((
                        x,
                        y,
                        !large && town.fenced_cell_ok(d, *clear, x, y, terrain),
                    ));
                }
            }
            return out;
        }
        if self.road.is_some() {
            // `FUN_14010a660`: the road's cells, or its ends marked invalid.
            let n = town.cells.size as i32;
            let b = if b < 0 { a } else { b };
            // Without a road, the path over any land (`ends | 0xff7`) shows where it fails:
            // each cell green where a road may cross it and no one is on it, else red.
            let path = self.road_cells(town, a, b).or_else(|| {
                let (ia, ib) = (a as usize, b as usize);
                let ends = (1u32 << (town.cells.types[ia].0 & 0x1f))
                    | (1u32 << (town.cells.types[ib].0 & 0x1f));
                town.agents
                    .road_path(
                        &town.cells,
                        a.min(b),
                        a.max(b),
                        ends | 0xff7,
                        self.manhattan,
                    )
                    .filter(|p| !p.is_empty())
            });
            return match path {
                Some(path) => path
                    .into_iter()
                    .map(|c| {
                        let i = c as usize;
                        let t = 1u32 << (town.cells.types[i].0 & 0x1f);
                        let ok = t & ROAD_MASK != 0 && town.cells.occupants[i].is_none();
                        (c % n, c / n, ok)
                    })
                    .collect(),
                None => vec![(a % n, a / n, false), (b % n, b / n, false)],
            };
        }
        let b = if b < 0 { a } else { b };
        let n = town.cells.size as i32;
        let (x0, y0, x1, y1) = self.rect(n, a, b);
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let e = Self::entity_at(town, (y * n + x) as usize);
                let acts = self.marks(town, terrain, e, x, y)
                    || self.clears_road(town, (y * n + x) as usize)
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
    fn drags_report_their_size_count_and_message() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let world = crate::view::world::World::generate(
            &archive,
            &crate::view::world::WorldParams {
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
        let drag = |tool: &mut RemovalTool, a: i32, b: i32| {
            tool.dragging = true;
            tool.start = a;
            tool.end = b;
        };
        let mut clear =
            RemovalTool::start(&archive, &mut town, r"Template\ClearTree.rsc:resource").unwrap();
        assert!(clear.drag_info(&mut town, &terrain).is_none());
        drag(&mut clear, (y - 2) * n + x - 3, (y + 2) * n + x + 3);
        let info = clear.drag_info(&mut town, &terrain).unwrap();
        assert_eq!(
            (info.mode, info.width, info.height, info.count),
            (0, 7, 5, 1)
        );
        // A tutorial's RestrictPlayer cells: only that drag is valid; any other fails.
        clear.restrict = Some([x - 3, y - 2, x + 3, y + 2]);
        assert_eq!(clear.drag_info(&mut town, &terrain).unwrap().message, 0);
        clear.restrict = Some([x - 3, y - 2, x + 3, y + 3]);
        assert_eq!(clear.drag_info(&mut town, &terrain).unwrap().message, 3);
        let before = town.placed.len();
        assert_eq!(clear.apply(&archive, &mut town, &terrain), DragSound::Error);
        assert_eq!(town.placed.len(), before);
        clear.restrict = None;
        // A road from a free cell 6 along: its length, the stone per cell.
        let mut road =
            RemovalTool::start(&archive, &mut town, r"Template\StoneRoad.rsc:resource").unwrap();
        let free = (0..n * n)
            .find(|&i| {
                (0..7).all(|k| {
                    let c = (i + k) as usize;
                    (i + k) % n >= k
                        && town.cells.types[c].0 == 2
                        && town.cells.occupants[c].is_none()
                        && town.cells.objects[c].is_none()
                })
            })
            .unwrap();
        drag(&mut road, free, free + 6);
        let info = road.drag_info(&mut town, &terrain).unwrap();
        assert_eq!(
            (info.mode, info.width, info.count, info.message),
            (1, 7, 7, 0)
        );
        // No road to the water: no size, message 1.
        let water = (0..n * n)
            .find(|&i| 1u32 << (town.cells.types[i as usize].0 & 0x1f) & ROAD_ENDS != 0)
            .unwrap();
        drag(&mut road, free, water);
        let info = road.drag_info(&mut town, &terrain).unwrap();
        assert_eq!((info.mode, info.message), (2, 1));
        // Hovering a free cell before a drag (`FUN_14010a660`): that cell, green.
        road.dragging = false;
        let cells = road.cells(&mut town, &terrain, free);
        assert_eq!(cells, vec![(free % n, free / n, true)], "hover");
        // A drag along free cells: every cell green.
        drag(&mut road, free, free + 6);
        let cells = road.cells(&mut town, &terrain, free);
        assert_eq!(cells.len(), 7);
        assert!(cells.iter().all(|c| c.2), "{cells:?}");
        // Drawn green with frame 1, not as a removal's red marks.
        let overlay = road.overlay(&mut town, &terrain, free);
        assert!(overlay
            .iter()
            .all(|c| c.argb == 0xff00_ff00 && c.frame == 1));
        // To the water: the way there shown, red where a road cannot go.
        drag(&mut road, free, water);
        let overlay = road.overlay(&mut town, &terrain, free);
        assert!(overlay.iter().any(|c| c.argb == 0xffff_0000));
    }

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
        let world = crate::view::world::World::generate(
            &archive,
            &crate::view::world::WorldParams {
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
