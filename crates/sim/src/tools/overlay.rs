//! The cells and lines a tool asks the overlay to draw (the overlay itself is the game app's
//! `gfx::selection_overlay`).

// +0xc; +0x10; +8
/// One cell of the overlay: map cell, strip frame, ARGB colour and the frame's rotation in
/// degrees (the placement tool's arrows, 0 for the drag tools).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayCell {
    pub x: i32,
    pub y: i32,
    pub frame: i32,
    pub argb: u32,
    pub rotation: i32,
}

impl OverlayCell {
    /// `(-(acts) & 3, acts ? 0xffff0000: 0x8000ff00)`.
    ///
    /// Original: `FUN_140109520` (entry).
    pub fn removal(x: i32, y: i32, acts: bool) -> OverlayCell {
        OverlayCell {
            x,
            y,
            frame: if acts { 3 } else { 0 },
            argb: if acts { 0xffff_0000 } else { 0x8000_ff00 },
            rotation: 0,
        }
    }
}

// +0x14; +0xc; +0x10; +0x1c; +0; +8
/// One line of the overlay (an entry of the overlay's first list, 0x20 bytes): from cell `a` to
/// cell `b`, strip frame, ARGB colour, rotation (0 here) and half its width.
///
/// Original: `FUN_140105d90`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayLine {
    pub a: (i32, i32),
    pub b: (i32, i32),
    pub frame: i32,
    pub argb: u32,
    pub width: f32,
}
