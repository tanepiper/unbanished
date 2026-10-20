//! A map (`start::map`) with the resources it is drawn with: the terrain mesh and textures.

use unbanished_formats::game::{EnvironmentDescription, TerrainGenerator};
use unbanished_formats::pkg::Archive;
use unbanished_formats::types::{image_buffer, material_instance, ImageBuffer};
use unbanished_formats::Result;
use unbanished_game::terrain::{HeightGrid, TerrainMesh};

use crate::start::map::{with_object, Map};

pub use crate::start::world_params::WorldParams;

pub struct TerrainTextures {
    pub grass: ImageBuffer,
    pub dirt: ImageBuffer,
    pub snow: ImageBuffer,
    pub rock: ImageBuffer,
}

/// Everything needed to draw a terrain: generated (New Game) or loaded (menu scenes, saves).
pub struct Terrain {
    /// Cells per side.
    pub size: usize,
    /// `(size + 1)²` saved height samples (`grid × 256`).
    pub heights: Vec<i16>,
    pub mesh: TerrainMesh,
    /// `size²` texels of `[dirt, rock, light, snow]` bytes (BGRA8 in memory).
    pub surface: Vec<u8>,
    pub textures: TerrainTextures,
    /// Bumped when the heights change in play (tunnels), for the renderer to re-upload.
    pub revision: u64,
}

impl Terrain {
    /// Build from saved heights and surface bytes, with textures from the terrain material.
    pub fn new(
        archive: &Archive,
        material: &str,
        size: usize,
        heights: Vec<i16>,
        surface: Vec<u8>,
    ) -> Result<Terrain> {
        let mesh = TerrainMesh::build(
            &HeightGrid {
                width: size + 1,
                heights: &heights,
            },
            size,
        );
        let material = material_instance(&archive.read_ref(&with_object(material))?)?;
        let textures = TerrainTextures {
            grass: texture(archive, &material.textures, "grass")?,
            dirt: texture(archive, &material.textures, "dirt")?,
            snow: texture(archive, &material.textures, "snow")?,
            rock: texture(archive, &material.textures, "rock")?,
        };
        Ok(Terrain {
            size,
            heights,
            mesh,
            surface,
            textures,
            revision: 0,
        })
    }

    /// The heights changed (TerraformComponent): the mesh follows (`FUN_1400b0620` marks the
    /// terrain's chunks for rebuilding).
    pub fn reshaped(&mut self) {
        self.mesh = TerrainMesh::build(
            &HeightGrid {
                width: self.width(),
                heights: &self.heights,
            },
            self.size,
        );
        self.revision += 1;
    }

    pub fn width(&self) -> usize {
        self.size + 1
    }

    /// Float height at the nearest cell corner (map-centred cell coordinates).
    pub fn height_at(&self, x: f32, y: f32) -> f32 {
        let half = (self.size / 2) as f32;
        let cx = ((x + half).round() as isize).clamp(0, self.size as isize) as usize;
        let cy = ((y + half).round() as isize).clamp(0, self.size as isize) as usize;
        self.heights[cy * self.width() + cx] as f32 / 256.0
    }
}

pub struct World {
    pub profile: TerrainGenerator,
    pub terrain: Terrain,
    pub environment: EnvironmentDescription,
    /// MapComponent `+0x1e0`: the river centre lines in map coordinates. `FUN_1400b1980` fills
    /// list `i` with the positions of generator path `i`'s 28-byte records, last to first
    /// (verified against the saved paths of a generated map).
    pub river_paths: Vec<Vec<[f32; 2]>>,
    /// The rivers' and lakes' surface (`FUN_1400b0730`).
    pub water: unbanished_game::terrain::WaterMesh,
}

fn texture(
    archive: &Archive,
    slots: &[(String, Option<String>)],
    slot: &str,
) -> Result<ImageBuffer> {
    let reference = slots
        .iter()
        .find(|(name, _)| name == slot)
        .and_then(|(_, r)| r.clone())
        .ok_or_else(|| {
            unbanished_formats::Error::Format(format!("terrain material has no {slot}"))
        })?;
    image_buffer(&archive.read_ref(&with_object(&reference))?)
}

impl World {
    pub fn generate(archive: &Archive, params: &WorldParams) -> Result<World> {
        World::from_map(archive, Map::generate(archive, params)?)
    }

    /// A saved game's world: its stored map with its drawing data.
    pub fn from_save(archive: &Archive, saved: &crate::saves::saved::SavedGame) -> Result<World> {
        World::from_map(archive, Map::from_save(archive, saved)?)
    }

    /// A map with its terrain mesh and textures.
    pub fn from_map(archive: &Archive, map: Map) -> Result<World> {
        let terrain = Terrain::new(
            archive,
            &map.material,
            map.heights.size,
            map.heights.values,
            map.surface,
        )?;
        Ok(World {
            profile: map.profile,
            terrain,
            environment: map.environment,
            river_paths: map.river_paths,
            water: map.water,
        })
    }
}
