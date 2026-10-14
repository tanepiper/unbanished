//! CPU-side scene content beyond terrain and placed meshes: the sky, the river/lake surface and
//! the tiled terrain decals (roads, fields, footprints). Loaded from a `MapComponent` block, which
//! menu scenes and save games share, so the in-game renderer can use the same data.

use banished_formats::menu_scene::{Decal, MapComponent};
use banished_formats::pkg::Archive;
use banished_formats::types::{graphics_mesh, image_buffer, material_instance, ImageBuffer};

pub(crate) fn with_object(reference: &str) -> String {
    let path = reference.replace('\\', "/");
    if path.contains(':') {
        path
    } else {
        format!("{path}:resource")
    }
}

fn image(archive: &Archive, reference: Option<&str>) -> Option<ImageBuffer> {
    let r = reference?;
    image_buffer(&archive.read_ref(&with_object(r)).ok()?)
        .map_err(|e| log::warn!("{r}: {e}"))
        .ok()
}

/// Texture slots of a material instance and its base material.
fn material(archive: &Archive, reference: &str) -> Option<(String, Vec<(String, Option<String>)>)> {
    let mi = material_instance(&archive.read_ref(&with_object(reference)).ok()?)
        .map_err(|e| log::warn!("{reference}: {e}"))
        .ok()?;
    Some((mi.material.unwrap_or_default(), mi.textures))
}

fn slot(textures: &[(String, Option<String>)], name: &str) -> Option<String> {
    textures
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, r)| r.clone())
}

/// `Models\Sky\SkyMesh.rsc`: a half dome of radius 0.5 sampled as a cube map (`SkyMaterial`).
pub struct SkyData {
    /// 24-byte static vertices and u16 indices.
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub index_count: u32,
    pub cube: ImageBuffer,
}

impl SkyData {
    pub fn load(archive: &Archive, mesh_ref: &str) -> Option<SkyData> {
        let mesh = graphics_mesh(&archive.read_ref(&with_object(mesh_ref)).ok()?).ok()?;
        let part = mesh.parts.first()?;
        let range = part.ranges.first()?;
        let (index_data, vertex_data, _) = part.geometry(range);
        let mi = mesh.material_instances.first()?.first()?.clone()?;
        let (_, textures) = material(archive, &mi)?;
        let cube = image(archive, slot(&textures, "clear").as_deref())?;
        let first = range.start_index as usize * 2;
        let mut indices = index_data[first..first + range.index_count as usize * 2].to_vec();
        indices.resize(indices.len().next_multiple_of(4), 0);
        Some(SkyData {
            vertices: vertex_data.to_vec(),
            indices,
            index_count: range.index_count as u32,
            cube,
        })
    }
}

/// The river and lake surface (`MapComponent` water mesh, `RiverMaterial` / `LakeMaterial`).
pub struct WaterData {
    /// 16-byte vertices: `i16 x, y` (1/48 cell), `i16 z` (1/256), `i16 w`, `i16 u, v`
    /// (1/1024), `u8 normal xyz, 0`; the normal's z is written to alpha.
    pub vertices: Vec<u8>,
    /// u16 indices, local to each range's first vertex.
    pub indices: Vec<u8>,
    /// `(first index, first vertex, vertex count, index count)`.
    pub rivers: Vec<[i32; 4]>,
    pub lakes: Vec<[i32; 4]>,
    pub river_water: ImageBuffer,
    pub lake_water: ImageBuffer,
    /// `highlight` cube map (`RiverReflectTexture`).
    pub highlight: ImageBuffer,
    /// Water surface height (world units) for the planar reflection.
    pub level: f32,
}

impl WaterData {
    pub fn load(archive: &Archive, map: &MapComponent) -> Option<WaterData> {
        if map.water_vertices.is_empty() {
            return None;
        }
        let river = material(archive, map.river_material.as_deref()?)?;
        let lake = material(archive, map.lake_material.as_deref()?)?;
        let river_water = image(archive, slot(&river.1, "water").as_deref())?;
        let lake_water = image(archive, slot(&lake.1, "water").as_deref())?;
        let highlight = image(archive, slot(&river.1, "highlight").as_deref())?;
        let n = map.water_vertices.len() as f32;
        let level = map
            .water_vertices
            .iter()
            .map(|v| i16::from_le_bytes([v[4], v[5]]) as f32 / 256.0)
            .sum::<f32>()
            / n;
        let mut indices: Vec<u8> = map
            .water_indices
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        indices.resize(indices.len().next_multiple_of(4), 0);
        Some(WaterData {
            vertices: map.water_vertices.iter().flatten().copied().collect(),
            indices,
            rivers: map.river_ranges.clone(),
            lakes: map.lake_ranges.clone(),
            river_water,
            lake_water,
            highlight,
            level,
        })
    }
}

/// Which decal shader a decal's material uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecalKind {
    /// `TerrainTiledDecal` / `TerrainTiledSnowDecal`: roads, yards, fields (diffuse /16, mask /4096).
    Tiled,
    /// `TerrainDecalBlend`: building footprints blended into a triplanar ground texture.
    Blend,
    /// `TerrainDecal` / `TerrainDamageDecal`.
    Plain,
}

pub struct DecalData {
    pub kind: DecalKind,
    /// 20-byte vertices: `i16 x, y, z (1/256), alpha`, `i16 uv ×4`, `u8 normal ×4`.
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub index_count: u32,
    pub diffuse: Option<ImageBuffer>,
    /// `tiles` (Tiled) or `blend` (Blend).
    pub mask: Option<ImageBuffer>,
    pub snow: Option<ImageBuffer>,
}

impl DecalData {
    /// Load every decal of a map, sharing textures between decals with the same material.
    pub fn load_all(archive: &Archive, map: &MapComponent) -> Vec<DecalData> {
        let mut materials = std::collections::HashMap::new();
        map.decals
            .iter()
            .chain(&map.extra_decals)
            .filter_map(|d| Self::load(archive, d, &mut materials))
            .collect()
    }

    #[allow(clippy::type_complexity)]
    fn load(
        archive: &Archive,
        d: &Decal,
        cache: &mut std::collections::HashMap<
            String,
            Option<(
                DecalKind,
                Option<ImageBuffer>,
                Option<ImageBuffer>,
                Option<ImageBuffer>,
            )>,
        >,
    ) -> Option<DecalData> {
        if d.vertices.is_empty() || d.indices.is_empty() {
            return None;
        }
        let reference = d.material.clone()?;
        let (kind, diffuse, mask, snow) = cache
            .entry(reference.clone())
            .or_insert_with(|| {
                let (base, textures) = material(archive, &reference)?;
                let kind = if base.contains("TiledDecal") || base.contains("TiledSnowDecal") {
                    DecalKind::Tiled
                } else if base.contains("DecalBlend") {
                    DecalKind::Blend
                } else {
                    DecalKind::Plain
                };
                let mask_slot = if kind == DecalKind::Blend {
                    "blend"
                } else {
                    "tiles"
                };
                Some((
                    kind,
                    image(archive, slot(&textures, "diffuse").as_deref()),
                    image(archive, slot(&textures, mask_slot).as_deref()),
                    image(archive, slot(&textures, "snow").as_deref()),
                ))
            })
            .clone()?;
        let mut indices: Vec<u8> = d.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        indices.resize(indices.len().next_multiple_of(4), 0);
        Some(DecalData {
            kind,
            vertices: d.vertices.iter().flatten().copied().collect(),
            indices,
            index_count: d.indices.len() as u32,
            diffuse,
            mask,
            snow,
        })
    }
}

/// Everything beyond terrain and models that a scene draws.
#[derive(Default)]
pub struct SceneExtras {
    pub sky: Option<SkyData>,
    pub water: Option<WaterData>,
    pub decals: Vec<DecalData>,
}

impl SceneExtras {
    pub fn from_map(archive: &Archive, map: &MapComponent, sky: Option<&str>) -> SceneExtras {
        SceneExtras {
            sky: sky.and_then(|s| SkyData::load(archive, s)),
            water: WaterData::load(archive, map),
            decals: DecalData::load_all(archive, map),
        }
    }
}
