//! What a tool or a window points at: a placed object or a citizen.

use glam::{Mat4, Vec3};
use unbanished_formats::skeleton::Transform;

use crate::town::StartingTown;
use crate::transform::placement_matrix;

/// A placed object or a citizen, by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Entity {
    Placed(usize),
    Citizen(usize),
}

impl Entity {
    pub fn template<'a>(self, town: &'a StartingTown) -> &'a str {
        match self {
            Self::Placed(i) => &town.templates[town.placed[i].template].reference,
            Self::Citizen(_) => "Template/Citizen.rsc:resource",
        }
    }
    pub fn model(self, town: &StartingTown) -> Option<(&str, Mat4)> {
        match self {
            Self::Placed(i) => {
                let p = town.placed.get(i)?;
                let r = town.templates[p.template]
                    .model
                    .as_ref()?
                    .variants
                    .get(p.variant)?
                    .meshes
                    .get(p.mesh)?
                    .as_deref()?;
                Some((
                    r,
                    Mat4::from_cols_array(&placement_matrix(
                        p.position,
                        [p.scale[0], p.scale[1], p.scale[2]],
                        p.yaw,
                    )),
                ))
            }
            Self::Citizen(i) => {
                let c = town.citizens.get(i)?;
                let r = town
                    .citizen_model
                    .as_ref()?
                    .variants
                    .get(if c.male { 0 } else { 1 })?
                    .meshes
                    .first()?
                    .as_deref()?;
                let s = if c.is_adult(&town.rules) { 1. } else { 0.66 };
                Some((
                    r,
                    Mat4::from_cols_array(&placement_matrix(
                        c.position,
                        [s; 3],
                        town.agents.characters[i].yaw,
                    )),
                ))
            }
        }
    }
    pub fn variant_stage(self, town: &StartingTown) -> (usize, usize) {
        match self {
            Self::Placed(i) => (town.placed[i].variant, town.placed[i].mesh),
            Self::Citizen(i) => (usize::from(!town.citizens[i].male), 0),
        }
    }
    pub fn position(self, town: &StartingTown) -> Vec3 {
        Vec3::from(match self {
            Self::Placed(i) => town.placed[i].position,
            Self::Citizen(i) => town.citizens[i].position,
        })
    }
    pub fn picking_transform(self, town: &StartingTown) -> Transform {
        let yaw = match self {
            Self::Placed(i) => town.placed[i].yaw,
            Self::Citizen(i) => town.agents.characters[i].yaw,
        };
        Transform {
            rotation: glam::Quat::from_rotation_z(yaw.to_radians()).to_array(),
            translation: self.position(town).to_array(),
        }
    }
}
