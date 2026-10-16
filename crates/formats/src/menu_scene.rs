//! `MenuScene` (`Game/MenuScene/Scene0..9.rsc`): the pre-built 3D scenes behind the main menu, and the
//! MapComponent block they embed (the same serializer saves use).
//!
//! Serializers: MenuScene `FUN_1400a8b00` (version 2), MapComponent `FUN_1400b59a0` (version 6)
//! and `FUN_1400b4670` (version 4), camera `FUN_14009dd90`, placements `FUN_14001c550`, decals
//! `FUN_140020900` / `FUN_140020320`. Layout notes: docs/MENU-SCENE.md.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// A tiled decal (roads, fields, building footprints) drawn over the terrain.
#[derive(Debug, Clone)]
pub struct Decal {
    pub material: Option<String>,
    pub ints: [i32; 5],
    pub byte: u8,
    pub int6: i32,
    pub flag7: bool,
    pub floats: [f32; 5],
    pub flag13: bool,
    pub flag_v2: bool,
    /// 20-byte vertices: `i16 x4, i16 x4, u8 x4`.
    pub vertices: Vec<[u8; 20]>,
    pub indices: Vec<u16>,
}

/// The serialized terrain and water of a map (`MapComponent`).
#[derive(Debug, Clone)]
pub struct MapComponent {
    /// `+0xc0..+0xd4`: size (2n), n, tile cells (32), tile vertices (33), width (2n+1), 33².
    pub dims: [i32; 6],
    pub heights: Vec<i16>,
    pub terrain_material: Option<String>,
    /// `size²` packed texels (see the terrain surface image, `FUN_1400d4910`).
    pub surface: Vec<u8>,
    /// River and lake mesh: 16-byte vertices (`i16 x, y, z, w; i16 u, v; u8 x4`).
    pub water_vertices: Vec<[u8; 16]>,
    pub water_indices: Vec<u16>,
    pub river_material: Option<String>,
    pub lake_material: Option<String>,
    /// Draw ranges `(first index, first vertex, vertex count, index count)` per river and lake.
    pub river_ranges: Vec<[i32; 4]>,
    pub lake_ranges: Vec<[i32; 4]>,
    /// River centre lines.
    pub river_paths: Vec<Vec<[f32; 2]>>,
    pub decals: Vec<Decal>,
    pub extra_decals: Vec<Decal>,
    pub ints: Vec<i32>,
    pub decal_materials: Vec<Option<String>>,
}

/// One placed mesh: position, rotation and scale, then mesh and material table indices.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub floats: [f32; 7],
    pub mesh: i16,
    pub material: i16,
}

#[derive(Debug, Clone, Copy)]
pub struct SceneCamera {
    /// Quaternion (x, y, z, w) then position.
    pub rotation: [f32; 4],
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub floats: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct ParticlePlacement {
    pub position: [f32; 3],
    pub yaw: f32,
    pub system: i32,
}

#[derive(Debug, Clone)]
pub struct MenuScene {
    pub name: String,
    pub map: MapComponent,
    pub camera: SceneCamera,
    pub placements: Vec<Placement>,
    pub meshes: Vec<Option<String>>,
    pub sky: Option<String>,
    pub vector: [f32; 4],
    /// Sun, ambient and fog colours (`0xAARRGGBB`), as in `EnvironmentMonth`.
    pub colors: [u32; 3],
    /// Fog start, fog end, fog maximum, sun yaw, sun pitch (inferred from the draw code).
    pub floats: [f32; 5],
    pub effect: Option<String>,
    pub effect_value: f32,
    pub particles: Vec<ParticlePlacement>,
    pub particle_systems: Vec<Option<String>>,
}

fn f32s<const N: usize>(r: &mut Reader) -> Result<[f32; N]> {
    let mut out = [0.0; N];
    for v in &mut out {
        *v = r.f32()?;
    }
    Ok(out)
}

fn i16(r: &mut Reader) -> Result<i16> {
    Ok(i16::from_le_bytes(r.bytes(2)?.try_into().unwrap()))
}

fn u16s(r: &mut Reader) -> Result<Vec<u16>> {
    let n = r.count()?;
    Ok(r.bytes(n * 2)?
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect())
}

fn refs(r: &mut Reader) -> Result<Vec<Option<String>>> {
    (0..r.count()?).map(|_| r.external_ref()).collect()
}

fn ranges(r: &mut Reader) -> Result<Vec<[i32; 4]>> {
    (0..r.count()?)
        .map(|_| Ok([r.i32()?, r.i32()?, r.i32()?, r.i32()?]))
        .collect()
}

/// `FUN_140020900`, with the enclosing serializer's version.
fn decals(r: &mut Reader, version: u32) -> Result<Vec<Decal>> {
    (0..r.count()?)
        .map(|_| {
            let material = r.external_ref()?;
            let ints = [r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?];
            let byte = r.u8()?;
            let int6 = r.i32()?;
            let flag7 = r.bool()?;
            let floats = f32s::<5>(r)?;
            let flag13 = r.bool()?;
            let flag_v2 = if version > 1 { r.bool()? } else { false };
            let (vertices, indices) = if version > 0 {
                let n = r.count()?;
                let vertices = (0..n)
                    .map(|_| Ok(r.bytes(20)?.try_into().unwrap()))
                    .collect::<Result<_>>()?;
                (vertices, u16s(r)?)
            } else {
                (Vec::new(), Vec::new())
            };
            Ok(Decal {
                material,
                ints,
                byte,
                int6,
                flag7,
                floats,
                flag13,
                flag_v2,
                vertices,
                indices,
            })
        })
        .collect()
}

/// `FUN_1400b59a0`: the MapComponent block.
pub fn read_map_component(r: &mut Reader) -> Result<MapComponent> {
    r.block(|r, version| {
        let mut dims = [0; 6];
        for d in &mut dims {
            *d = r.i32()?;
        }
        let n = r.count()?;
        let heights = r
            .bytes(n * 2)?
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let terrain_material = r.external_ref()?;
        let size = dims[0].max(0) as usize;
        let surface = r.bytes(size * size * 4)?.to_vec();
        let (water_vertices, water_indices) = if version < 3 {
            r.bool()?;
            let vb = r.count()?;
            let vertices = r
                .bytes(vb)?
                .chunks_exact(16)
                .map(|c| c.try_into().unwrap())
                .collect();
            r.bool()?;
            let ib = r.count()?;
            let indices = r
                .bytes(ib)?
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            (vertices, indices)
        } else {
            let n = r.count()?;
            let vertices = (0..n)
                .map(|_| Ok(r.bytes(16)?.try_into().unwrap()))
                .collect::<Result<_>>()?;
            (vertices, u16s(r)?)
        };
        let river_material = r.external_ref()?;
        let lake_material = r.external_ref()?;
        let river_ranges = ranges(r)?;
        let lake_ranges = ranges(r)?;
        let river_paths = (0..r.count()?)
            .map(|_| (0..r.count()?).map(|_| f32s::<2>(r)).collect())
            .collect::<Result<_>>()?;
        let decals = decals(r, version)?;
        let ints = if version > 3 {
            (0..r.count()?).map(|_| r.i32()).collect::<Result<_>>()?
        } else {
            Vec::new()
        };
        let decal_materials = if version > 5 { refs(r)? } else { Vec::new() };
        Ok(MapComponent {
            dims,
            heights,
            terrain_material,
            surface,
            water_vertices,
            water_indices,
            river_material,
            lake_material,
            river_ranges,
            lake_ranges,
            river_paths,
            decals,
            extra_decals: Vec::new(),
            ints,
            decal_materials,
        })
    })
}

/// `FUN_1400b4670`: the second decal block (replaces the decal lists).
fn read_map_decals(r: &mut Reader, map: &mut MapComponent) -> Result<()> {
    r.block(|r, version| {
        map.extra_decals = decals(r, version)?;
        map.decals = decals(r, version)?;
        if version > 3 {
            map.decal_materials = refs(r)?;
        }
        Ok(())
    })
}

/// `FUN_14009dd90`: the camera block (also in saves).
pub fn read_camera(r: &mut Reader) -> Result<SceneCamera> {
    r.block(|r, _| {
        Ok(SceneCamera {
            rotation: f32s::<4>(r)?,
            position: f32s::<3>(r)?,
            target: f32s::<3>(r)?,
            floats: f32s::<3>(r)?,
        })
    })
}

pub fn menu_scene(data: &[u8]) -> Result<MenuScene> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("MenuScene") {
            return format_err(format!("expected MenuScene, found {:#010x}", h.type_id));
        }
        r.block(|r, version| {
            let name = r.string()?;
            let mut map = read_map_component(r)?;
            if version > 0 {
                read_map_decals(r, &mut map)?;
            }
            let camera = read_camera(r)?;
            let placements = (0..r.count()?)
                .map(|_| {
                    Ok(Placement {
                        floats: f32s::<7>(r)?,
                        mesh: i16(r)?,
                        material: i16(r)?,
                    })
                })
                .collect::<Result<_>>()?;
            let meshes = refs(r)?;
            let sky = r.external_ref()?;
            let vector = f32s::<4>(r)?;
            let colors = [r.u32()?, r.u32()?, r.u32()?];
            let floats = f32s::<5>(r)?;
            let (mut effect, mut effect_value, mut particles, mut particle_systems) =
                (None, 0.0, Vec::new(), Vec::new());
            if version > 1 {
                effect = r.external_ref()?;
                effect_value = r.f32()?;
                particles = (0..r.count()?)
                    .map(|_| {
                        Ok(ParticlePlacement {
                            position: f32s::<3>(r)?,
                            yaw: r.f32()?,
                            system: r.i32()?,
                        })
                    })
                    .collect::<Result<_>>()?;
                particle_systems = refs(r)?;
            }
            Ok(MenuScene {
                name,
                map,
                camera,
                placements,
                meshes,
                sky,
                vector,
                colors,
                floats,
                effect,
                effect_value,
                particles,
                particle_systems,
            })
        })
    })?;
    Ok(value)
}
