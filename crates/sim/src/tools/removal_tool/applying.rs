//! Applying a drag: which entities it marks, cancels, clears or demolishes, the commands it gives, and the overlay of its cells.

use super::*;

impl RemovalTool {
    /// The entity a cell answers for: the occupant (`+0x00`), else the object (`+0x08`).
    pub(super) fn entity_at(town: &StartingTown, i: usize) -> Option<usize> {
        town.cells.entity_at(i).filter(|&e| town.placed[e].alive)
    }

    /// `FUN_140109d70`: mark this natural resource for removal.
    pub(super) fn marks(
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
    pub(super) fn cancels(&self, town: &StartingTown, e: Option<usize>) -> bool {
        let Some(e) = e else { return false };
        if !self.desc.cancel {
            return false;
        }
        Some(town.placed[e].template) == self.marker
            || (Self::has_destroy(town, e) && matches!(Self::shown_state(town, e), 3..=5))
    }

    /// Remove Roads (`+0x40`): a road cell (types 0, 1), a planned one or a road plan.
    pub(super) fn clears_road(&self, town: &StartingTown, i: usize) -> bool {
        if !self.desc.roads {
            return false;
        }
        let t = town.cells.types[i].0;
        t == 0
            || t == 1
            || town.cells.road_kind[i] != unbanished_game::work::road::NONE
            || town.construction.pending_roads.contains_key(&(i as i32))
    }

    pub(super) fn has_destroy(town: &StartingTown, e: usize) -> bool {
        town.construction
            .sites
            .get(&e)
            .is_some_and(|s| s.destroy.is_some())
    }

    /// The StateMachine's pending state, else its state (`+0x52`, `+0x50`).
    pub(super) fn shown_state(town: &StartingTown, e: usize) -> i8 {
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
    pub(super) fn demolishes(&self, town: &StartingTown, e: Option<usize>) -> bool {
        let Some(e) = e else { return false };
        self.desc.buildings
            && Some(town.placed[e].template) != self.marker
            && Self::has_destroy(town, e)
            && !matches!(Self::shown_state(town, e), 3..=5)
    }

    pub(super) fn command(
        town: &mut StartingTown,
        command: unbanished_game::work::professions::Command,
    ) {
        if !town.construction.commands.contains(&command) {
            town.construction.commands.push(command);
        }
    }

    /// `FUN_140133fa0` on the entity's WorkComponent: its work is registered.
    pub(super) fn work_of(town: &StartingTown, e: Option<usize>) -> Option<i32> {
        let site = town.construction.sites.get(&e?)?;
        town.construction
            .jobs
            .works
            .contains_key(&site.work_id)
            .then_some(site.work_id)
    }

    /// Applies the drag over the dragged rectangle: markers, roads, cancels, demolition, priority.
    /// In the road mode, every cell of the road is planned; the road decals follow the road
    /// revision (`road_decals.rs`).
    ///
    /// Original: `FUN_1400a1bd0`, `FUN_140109b40`, `FUN_140123470`, `FUN_14010a200`, `FUN_1400a1e30`,
    /// `FUN_140108ab0`.
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
                    unbanished_game::work::professions::Command::PlanRoad {
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
                            unbanished_game::work::professions::Command::ClearRoad {
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
                            unbanished_game::work::professions::Command::StateMessage {
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
                        unbanished_game::work::professions::Command::Demolish { entity },
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

    /// The overlay of [`Self::cells`]: the removal tools mark the cells they act on red with the
    /// cross; the building tools draw green where the cell takes them and red where not, the road's
    /// cells with frame 1, the bridge's spanned cells too, the areas' and fences' plain.
    ///
    /// Original: `FUN_140109520`, `FUN_14010a660`, `FUN_140100290`, `FUN_140101310`, `FUN_1400e1910`.
    pub fn overlay(
        &self,
        town: &mut StartingTown,
        terrain: &HeightGrid,
        hover: i32,
    ) -> Vec<crate::tools::overlay::OverlayCell> {
        use crate::tools::overlay::OverlayCell;
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

    /// The cells to draw, each with whether the drag would act on it.
    ///
    /// Original: `FUN_140109520`.
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
