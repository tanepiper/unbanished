//! Dragging: the drag's ends and rectangle, its information, its command, pressing, moving, releasing and cancelling, and its sounds.

use super::*;

impl RemovalTool {
    /// The drag's start and end cells while dragging (for the CreateBar).
    pub fn drag_ends(&self) -> Option<(i32, i32)> {
        (self.dragging && self.start >= 0 && self.end >= 0).then_some((self.start, self.end))
    }

    /// Off a RestrictPlayer setup's cells (`FUN_1401093c0`, `FUN_140108ab0`, `FUN_140100fb0`,
    /// `FUN_1400e0960`, `FUN_14010a4b0`, `FUN_14010a200`: message 3, the apply fails). Bridges
    /// are not held to it.
    pub(super) fn off_restrict(&self, n: i32, a: i32, b: i32) -> bool {
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

    // +0x70
    /// Slot for the CreateBar while dragging:
    /// - RemovalTool and DragPlacementTool (its check): the rectangle `W×H`, the requirements once;
    /// - FencedTool: the rectangle, the requirements once per fence cell (`2(W + H) − 4`); 1 too
    ///   small, 2 too large, 3 a cell that will not take it;
    /// - RoadTool: the road's length, the requirements per cell; without a road no size and message
    ///   1;
    /// - BridgeTool: the span's length, the requirements per cell; else 3.
    ///
    /// Original: `FUN_140101b80`, `FUN_1401093c0`, `FUN_140100fb0`, `FUN_1400e0960`, `FUN_14010a4b0`, `FUN_1401001c0`.
    pub fn drag_info(
        &self,
        town: &mut StartingTown,
        terrain: &HeightGrid,
    ) -> Option<crate::tools::drag_info::DragInfo> {
        use crate::tools::drag_info::DragInfo;
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

    /// The drag as it stands, for `PlayerCommand::Drag`: the template, the cells and the
    /// settings `apply` reads.
    pub fn drag_command(&self) -> crate::tools::drag::Drag {
        crate::tools::drag::Drag {
            template: self.reference.clone(),
            start: self.start,
            end: self.end,
            rotation: self.rotation,
            restrict: self.restrict,
            manhattan: self.manhattan,
        }
    }

    /// This tool with a drag's cells and settings, ready to `apply`.
    pub fn with_drag(&self, drag: &crate::tools::drag::Drag) -> RemovalTool {
        let mut tool = self.clone();
        tool.start = drag.start;
        tool.end = drag.end;
        tool.rotation = drag.rotation;
        tool.restrict = drag.restrict;
        tool.manhattan = drag.manhattan;
        tool
    }

    /// Apply a drag from cell `start` to cell `end` (tests; the game goes through
    /// `PlayerCommand::Drag`).
    #[cfg(any(test, feature = "test-support"))]
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
    pub fn press(&mut self, cell: i32) -> Option<DragStep> {
        self.held = true;
        if !self.dragging {
            self.start = cell;
            self.end = cell;
            if cell >= 0 {
                self.dragging = true;
                self.waiting = false;
                return self.track(cell).map(DragStep::Sound);
            }
            return None;
        }
        if self.waiting {
            // The second click applies.
            let step = if cell < 0 {
                DragStep::Sound(DragSound::Error)
            } else {
                self.end = cell;
                DragStep::Apply
            };
            self.dragging = false;
            self.waiting = false;
            return Some(step);
        }
        None
    }

    /// The cursor moved over `cell`; while MouseLeft is up (released off the map) the drag
    /// completes as soon as the cursor is back on it.
    pub fn motion(&mut self, cell: i32) -> Option<DragStep> {
        if !self.dragging {
            return None;
        }
        let sound = if cell >= 0 { self.track(cell) } else { None };
        if !self.waiting && !self.held {
            return self.finish(cell).or(sound.map(DragStep::Sound));
        }
        sound.map(DragStep::Sound)
    }

    /// MouseLeft released over `cell`.
    pub fn release(&mut self, cell: i32) -> Option<DragStep> {
        self.held = false;
        if !self.dragging || self.waiting {
            return None;
        }
        if cell >= 0 {
            self.track(cell);
        }
        self.finish(cell)
    }

    /// Stop a drag in progress; false when there was none.
    ///
    /// Original: `FUN_140101b10`.
    pub fn cancel(&mut self) -> bool {
        if !self.dragging {
            return false;
        }
        self.dragging = false;
        self.waiting = false;
        true
    }

    /// The end cell follows the cursor; a new one plays the move sound.
    pub(super) fn track(&mut self, cell: i32) -> Option<DragSound> {
        self.end = cell;
        if self.sounded != self.end {
            self.sounded = self.end;
            return Some(DragSound::Move);
        }
        None
    }

    /// The button is up while dragging: on the start cell wait for a click, off the map
    /// complain, else apply.
    pub(super) fn finish(&mut self, cell: i32) -> Option<DragStep> {
        if self.start == self.end {
            self.waiting = true;
            return None;
        }
        if cell < 0 {
            return Some(DragStep::Sound(DragSound::Error));
        }
        self.dragging = false;
        Some(DragStep::Apply)
    }

    pub(super) fn rect(&self, n: i32, a: i32, b: i32) -> (i32, i32, i32, i32) {
        let (ax, ay, bx, by) = (a % n, a / n, b % n, b / n);
        (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by))
    }
}
