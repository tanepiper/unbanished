//! A drag tool's apply as data, for commands and recordings.

use serde::{Deserialize, Serialize};

/// A drag: the tool's template, and the cells and settings it was applied with. The tool is the
/// one `PlayerCommand::StartTool` made for that template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drag {
    pub template: String,
    pub start: i32,
    pub end: i32,
    pub rotation: i32,
    pub restrict: Option<[i32; 4]>,
    pub manhattan: bool,
}
