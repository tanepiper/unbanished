//! A generated map: the New Game terrain pipeline (`FUN_1400b5700` → `FUN_1400b1980`) run with the
//! bit-exact ports in `banished-game`, plus the resources it is drawn with.

use banished_formats::game::{
    environment_description, terrain_generator, EnvironmentDescription, TerrainGenerator,
};
use banished_formats::pkg::Archive;
use banished_formats::types::{image_buffer, material_instance, ImageBuffer};
use banished_formats::Result;
use banished_game::terrain::{Generator, HeightGrid, SizeRecord, TerrainMesh};

pub struct WorldParams {
    pub town_name: String,
    pub terrain: String,
    pub climate: String,
    /// The selected `ClimateGenerator` (`Game\Climate\Fair.rsc:resource`).
    pub climate_ref: String,
    pub start_condition: String,
    /// The selected `MapGenerator` (`Game\\StartCondition\\Medium.rsc:resource`).
    pub start_condition_ref: String,
    /// 0 Small, 1 Medium, 2 Large.
    pub size: usize,
    pub seed: u32,
    /// The Disasters combo's first item (on): `Game +0xd0` starts 0, else `0xf0`
    /// (`FUN_14009d530`).
    pub disasters: bool,
    /// The scenario played (an index into `Game\Scenario\ScenarioList.rsc`, `FUN_1400b9a10`).
    pub scenario: Option<usize>,
}

impl Default for WorldParams {
    fn default() -> Self {
        WorldParams {
            town_name: "New Settlement".into(),
            terrain: "Valleys".into(),
            climate: "Fair".into(),
            climate_ref: r"Game\Climate\Fair.rsc:resource".into(),
            start_condition: "Medium".into(),
            start_condition_ref: String::new(),
            size: 0,
            seed: 290,
            disasters: true,
            scenario: None,
        }
    }
}

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
    pub water: banished_game::terrain::WaterMesh,
}

fn with_object(reference: &str) -> String {
    let path = reference.replace('\\', "/");
    if path.contains(':') {
        path
    } else {
        format!("{path}:resource")
    }
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
            banished_formats::Error::Format(format!("terrain material has no {slot}"))
        })?;
    image_buffer(&archive.read_ref(&with_object(&reference))?)
}

impl World {
    pub fn generate(archive: &Archive, params: &WorldParams) -> Result<World> {
        let profile = terrain_generator(
            &archive.read_ref(&format!("Game/Terrain/{}.rsc:resource", params.terrain))?,
        )?;
        let wire = profile.sizes[params.size.min(profile.sizes.len() - 1)];
        let start = web_time::Instant::now();
        let mut g = Generator::new(SizeRecord::from_wire(wire), params.seed);
        g.run(|_, _| {});
        let heights = g.heights();
        let surface = g
            .surface_map()
            .iter()
            .flat_map(|t| t.to_le_bytes())
            .collect();
        let size = g.size as usize;
        log::info!(
            "generated {} {} seed {} ({size} cells) in {:.2}s",
            profile.name,
            params.size,
            params.seed,
            start.elapsed().as_secs_f32()
        );
        let river_paths = g
            .paths
            .iter()
            .map(|p| p.iter().rev().map(|q| q.pos).collect())
            .collect();
        let water = banished_game::terrain::water_mesh(&g.paths, &g.lakes, &heights, size);
        let terrain = Terrain::new(archive, &profile.materials[0], size, heights, surface)?;
        let environment =
            environment_description(&archive.read_ref("Environment/Environment.rsc:resource")?)?;
        Ok(World {
            profile,
            terrain,
            environment,
            river_paths,
            water,
        })
    }
}
