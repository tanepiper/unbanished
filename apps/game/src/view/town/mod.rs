//! The town as drawn: the data its models and icons need, read once from the game data, and
//! the models and status icons built from the town (`StartingTown`). Presentation only: it reads
//! the town and never changes it.

mod icons;
mod models;

use std::collections::HashMap;

use unbanished_formats::game::{raw_material_description, StatusIconDescription};
use unbanished_formats::pkg::Archive;
use unbanished_formats::types::Bone;

use crate::gfx::models::{
    placement_matrix, GroupInstances, GroupMaterial, ModelInstance, SceneModels,
};
use crate::start::{citizen_tool_nodes, normalise, read, StartingTown};

/// What drawing the town needs beyond the town itself.
pub struct TownView {
    /// The citizens' status icons (`:statusicon`) and their models' heights, for the icons'
    /// places above heads.
    citizen_icons: Option<StatusIconDescription>,
    citizen_heights: [f32; 2],
    /// The citizen description's tool meshes, and the bones they hang from.
    pub(crate) tool_meshes: Vec<Option<String>>,
    pub(crate) citizen_bones: Vec<Option<(Vec<Bone>, usize)>>,
    /// What each raw material looks like when carried: its mesh and scale, read on first use.
    carry_models: std::cell::RefCell<HashMap<usize, Option<(String, f32)>>>,
}

impl TownView {
    /// Read what drawing `town` needs from the game data.
    pub fn new(archive: &Archive, town: &StartingTown) -> TownView {
        let citizen_ref = normalise(&town.citizen);
        let citizen_model = town.citizen_model.as_ref();
        let citizen_icons = read(archive, &citizen_ref.replace(":resource", ":statusicon"))
            .and_then(|d| unbanished_formats::game::status_icon_description(&d).ok());
        let citizen_heights = [0, 1].map(|v| {
            citizen_model
                .and_then(|m| m.variants.get(v)?.meshes.first()?.clone())
                .and_then(|r| read(archive, &normalise(&r)))
                .and_then(|d| unbanished_formats::types::graphics_mesh(&d).ok())
                .map_or(0.0, |m| m.bounds_max[2] - m.bounds_min[2])
        });
        TownView {
            citizen_icons,
            citizen_heights,
            tool_meshes: read(archive, &citizen_ref.replace(":resource", ":citizen"))
                .and_then(|d| unbanished_formats::game::citizen_description(&d).ok())
                .map(|c| c.list)
                .unwrap_or_default(),
            citizen_bones: citizen_model
                .map(|m| citizen_tool_nodes(archive, m))
                .unwrap_or_default(),
            carry_models: Default::default(),
        }
    }
}
