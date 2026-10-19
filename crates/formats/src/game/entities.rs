//! What every entity is made of: components, models, decals, characters and interaction points.

use super::*;

/// An entity template (`ComponentDescription`, serializer `FUN_14011dfc0`): the scheduling bucket
/// and the ordered component descriptions (`model`, `naturalresource`, `growth`, …).
#[derive(Debug, Clone)]
pub struct ComponentDescription {
    /// `+0x20`: manager bucket (default 6 = unscheduled).
    pub bucket: u32,
    /// `+0x28`: description references.
    pub descriptions: Vec<Option<String>>,
}

pub fn component_description(data: &[u8]) -> Result<ComponentDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ComponentDescription") {
            return format_err(format!(
                "expected ComponentDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let bucket = r.u32()?;
            let descriptions = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            Ok(ComponentDescription {
                bucket,
                descriptions,
            })
        })
    })?;
    Ok(value)
}

/// One model variant: an animation set and its alternative meshes.
#[derive(Debug, Clone)]
pub struct ModelVariant {
    pub animations: Option<String>,
    pub meshes: Vec<Option<String>>,
}

/// `ModelDescription` (serializer `FUN_14012a290`), read by the model component's setup
/// `FUN_140129f90`.
#[derive(Debug, Clone)]
pub struct ModelDescription {
    /// `+0x20` / `+0x28`, 0x18 bytes each.
    pub variants: Vec<ModelVariant>,
    /// `+0x30`: variant used unless `random_variant`.
    pub variant: i32,
    /// `+0x34`: mesh index within the variant (negative = no mesh).
    pub mesh: i32,
    /// `+0x38`: pick the variant with `Rand(variant count)`.
    pub random_variant: bool,
}

pub fn model_description(data: &[u8]) -> Result<ModelDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ModelDescription") {
            return format_err(format!(
                "expected ModelDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let mut variants = Vec::new();
            for _ in 0..r.count()? {
                let animations = r.external_ref()?;
                let meshes = (0..r.count()?)
                    .map(|_| r.external_ref())
                    .collect::<Result<_>>()?;
                variants.push(ModelVariant { animations, meshes });
            }
            Ok(ModelDescription {
                variants,
                variant: r.i32()?,
                mesh: r.i32()?,
                random_variant: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `DecalDescription` (serializer `FUN_140120e50`): the terrain decal a DecalComponent puts
/// over its entity's map rectangle (`Template\X.rsc:decal`).
#[derive(Debug, Clone, PartialEq)]
pub struct DecalDescription {
    /// `+0x20`: the alpha until the entity is built.
    pub alpha: f32,
    /// `+0x28`: materials; the component's `+0x28` picks one.
    pub materials: Vec<Option<String>>,
    /// `+0x48`: the texture's size in rectangle units.
    pub scale: f32,
    /// `+0x38`: a texture rectangle (`u0, v0, u1, v1`) per model variant.
    pub rects: Vec<[f32; 4]>,
    /// `+0x24`: tiled (the road atlas by the rectangle's edges) instead of one picture.
    pub tiled: bool,
    /// `+0x25`.
    pub flag: bool,
}

pub fn decal_description(data: &[u8]) -> Result<DecalDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("DecalDescription") {
            return format_err(format!(
                "expected DecalDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let alpha = r.f32()?;
            let materials = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            let scale = r.f32()?;
            let rects = (0..r.count()?)
                .map(|_| r.f32s::<4>())
                .collect::<Result<_>>()?;
            Ok(DecalDescription {
                alpha,
                materials,
                scale,
                rects,
                tiled: r.bool()?,
                flag: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `CharacterDescription` (serializer `FUN_14010f2a0`, defaults from constructor `FUN_14010f3e0`):
/// how a walking entity moves (CharacterComponent update `FUN_14010e990`, MoveTo `FUN_1400cda40`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterDescription {
    /// `+0x20` (default 0.25).
    pub radius: f32,
    /// `+0x24`: added to the heading when the model is turned (default 0).
    pub yaw_offset: f32,
    /// `+0x28`: fraction of the remaining turn kept each update (default 0.85).
    pub turn_keep: f32,
    /// `+0x2c`: animation blend time passed to `PlayAnimation` (default 0.1).
    pub blend: f32,
    /// `+0x30`: speed scale while following another character (default 1).
    pub follow_speed: f32,
    /// `+0x34`: speed scale on cell types 1 and 9 (roads; default 1).
    pub road_speed: f32,
    /// `+0x38`: speed scale on cell type 0 (default 1).
    pub type0_speed: f32,
    /// `+0x3c` / `+0x40`: hold a fixed height instead of following the ground.
    pub fixed_height: bool,
    pub height: f32,
    /// `+0x44`: lowest height the ground clamp allows (default -10).
    pub min_height: f32,
}

pub fn character_description(data: &[u8]) -> Result<CharacterDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CharacterDescription") {
            return format_err(format!(
                "expected CharacterDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(CharacterDescription {
                radius: r.f32()?,
                yaw_offset: r.f32()?,
                turn_keep: r.f32()?,
                blend: r.f32()?,
                follow_speed: r.f32()?,
                road_speed: r.f32()?,
                type0_speed: r.f32()?,
                fixed_height: r.bool()?,
                height: r.f32()?,
                min_height: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

/// A named point of a `Pathfinding::PointList` (position in model space, yaw in degrees).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedPoint {
    pub name: String,
    pub position: [f32; 3],
    pub yaw: f32,
}

/// `Pathfinding::PointList` (Runtime serializer `0x180111f70`): mesh name, a second string,
/// path points (`+0x40`) and named points (`+0x30`).
#[derive(Debug, Clone)]
pub struct PointList {
    pub mesh: String,
    pub source: String,
    pub path_points: Vec<NamedPoint>,
    pub points: Vec<NamedPoint>,
}

pub fn point_list(data: &[u8]) -> Result<PointList> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("PointList") {
            return format_err(format!("expected PointList, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            let mesh = r.string()?;
            let source = r.string()?;
            let list = |r: &mut Reader| -> Result<Vec<NamedPoint>> {
                (0..r.count()?)
                    .map(|_| {
                        Ok(NamedPoint {
                            name: r.string()?,
                            position: r.f32s::<3>()?,
                            yaw: r.f32()?,
                        })
                    })
                    .collect()
            };
            let path_points = list(r)?;
            let points = list(r)?;
            Ok(PointList {
                mesh,
                source,
                path_points,
                points,
            })
        })
    })?;
    Ok(value)
}

/// `InteractDescription` (serializer `FUN_1401289d0`): the point list whose named points
/// "use", "build" and "create" become the three interaction categories (`+0x28/+0x38/+0x48`).
#[derive(Debug, Clone)]
pub struct InteractDescription {
    pub points: Option<String>,
}

pub fn interact_description(data: &[u8]) -> Result<InteractDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("InteractDescription") {
            return format_err(format!(
                "expected InteractDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(InteractDescription {
                points: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

/// Interaction points of one category (0 use, 1 build, 2 create) in point-list order.
pub fn interaction_points(list: &PointList, category: usize) -> Vec<&NamedPoint> {
    let name = ["use", "build", "create"][category];
    list.points.iter().filter(|p| p.name == name).collect()
}

impl Default for CharacterDescription {
    fn default() -> Self {
        CharacterDescription {
            radius: 0.25,
            yaw_offset: 0.0,
            turn_keep: 0.85,
            blend: 0.1,
            follow_speed: 1.0,
            road_speed: 1.0,
            type0_speed: 1.0,
            fixed_height: false,
            height: 0.0,
            min_height: -10.0,
        }
    }
}
