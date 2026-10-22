//! The removal tools (`RemovalTool: DragTool`): ClearTree, ClearStone, ClearIron, ClearResources,
//! ClearRoad, ClearBuildings, ClearCancel and ClearPriority, started by toolbar kind 1 for a
//! template with a `CreateRemovalDescription`. See docs/JOBS.md.
//!
//! - Start: keep the template's CreateRemovalDescription;
//! - Input (DragTool): MouseLeft starts a drag at the cell under the cursor;
//!   releasing on another cell applies it, releasing on the same cell waits for a second click; a
//!   new end cell plays the move sound;
//! - Cancel: a drag in progress stops;
//! - Apply: every cell of the rectangle, in rows;
//! - Draw: each cell of the drag (or the hovered cell), marked when the drag would do something
//!   there.
//!
//! The original runs the tool between simulation iterations; the port does too (markers are
//! activated before the next iteration, like placed buildings).
//!
//! RoadTool (`RoadTool: DragTool`, for a template with a `CreateRoadDescription`) shares the
//! DragTool input and runs here as the road mode, with its own start, apply and check (see
//! docs/ROADS.md).
//!
//! Original: `FUN_14010a0a0`, `FUN_140101b10`, `FUN_140108ab0`, `FUN_140109520`, `FUN_140101b80`,
//! `FUN_14010abb0`, `FUN_14010a200`, `FUN_14010a4b0`.
mod applying;
mod dragging;
mod starting;

// vtable 0x140168200; +0x20; +0x38; +0x30; +0x48; +0x68; +0x40; vtable 0x140168290

use unbanished_formats::game::{
    create_drag_description, create_removal_description, create_road_description,
    toolbar_description, CreateDragDescription, CreateRemovalDescription, ToolbarDescription,
};
use unbanished_formats::pkg::Archive;
use unbanished_game::terrain::HeightGrid;
use unbanished_game::world::placement::cell_admits;

use crate::town::StartingTown;

fn read(archive: &Archive, reference: &str) -> Option<Vec<u8>> {
    archive.read_ref(&reference.replace('\\', "/")).ok()
}

fn same_reference(a: &str, b: &str) -> bool {
    a.replace('/', "\\")
        .eq_ignore_ascii_case(&b.replace('/', "\\"))
}

/// What a press, move or release asks for: a sound, or the drag applied
/// ([`RemovalTool::apply`], through `PlayerCommand::Drag`), whose sound comes from that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragStep {
    Sound(DragSound),
    Apply,
}

/// What a press, move or release did (the caller plays the sound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSound {
    /// The move sound.
    ///
    /// Original: `FUN_1401050c0`.
    Move,
    /// The sound of an applied drag.
    ///
    /// Original: `FUN_1401055c0`.
    Thunk,
    /// The error sound.
    ///
    /// Original: `FUN_140105610`.
    Error,
}

#[derive(Clone)]
pub struct RemovalTool {
    /// `Template\ClearX.rsc:resource`.
    pub reference: String,
    desc: CreateRemovalDescription,
    /// The marker template (`desc +0x38`) and its CreateDragDescription.
    marker: Option<usize>,
    drag: Option<CreateDragDescription>,
    pub toolbar: Option<ToolbarDescription>,
    // +0x28; +0x29
    /// DragTool: dragging;: released on the start cell, waiting for a click.
    pub dragging: bool,
    waiting: bool,
    // +0x2c; +0x30; +0x34
    /// start cell, end cell, the end cell the move sound last played for (−1).
    pub start: i32,
    pub end: i32,
    sounded: i32,
    /// MouseLeft is held.
    held: bool,
    /// The road mode (RoadTool `+0x38`): the CreateRoadDescription's kind and material, and
    /// the cell type of the road template's MapDescription.
    road: Option<RoadSpec>,
    /// The bridge mode (BridgeTool `+0x38`): the CreateBridgeDescription and its parts.
    bridge: Option<crate::town::BridgeSpec>,
    /// The area mode (DragPlacementTool `+0x38`, `+0x40`): the template, its
    /// CreateDragDescription and the template whose entities may stand in the way.
    area: Option<AreaSpec>,
    /// The fenced mode (FencedTool): the CreateFencedDescription and its clear template.
    fenced: Option<(
        unbanished_formats::game::CreateFencedDescription,
        Option<usize>,
    )>,
    // +0x50 → +0x10
    /// The RotationSetting the fenced tool turns its shelter by.
    pub rotation: i32,
    // +0x80; +0x98; +0x9c
    /// A RestrictPlayer setup's cells (game UI, corners / as `(x0, y0, x1, y1)`): the drag must
    /// cover exactly them (a road must run between those corners).
    pub restrict: Option<[i32; 4]>,
    // +0x40
    /// RoadTool: the path's Manhattan heuristic, on while PathMethod (action `0x16`) is not held.
    ///
    /// Original: `FUN_14010ab60`.
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
pub const ROAD_ENDS: u32 = 0x5e0;

impl RemovalTool {}
