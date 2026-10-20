//! What a drag tool reports to the CreateBar while it drags.

// +0x70
/// What a DragTool's slot reports for the CreateBar.
///
/// Original: `FUN_140101b80`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragInfo {
    /// 0 `"WxH"`, 1 `"%i"` of the width, 2 no size (and the counts left as they are).
    pub mode: u8,
    pub width: i32,
    pub height: i32,
    /// The multiplier of the build requirements (cells of road, bridge or fence).
    pub count: i32,
    /// The ToolbarDescription message shown; 0 is valid (the icon white, else red).
    pub message: usize,
}
