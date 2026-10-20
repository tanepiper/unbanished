//! A town's map without its drawing data: the New Game terrain pipeline (`FUN_1400b5700` →
//! `FUN_1400b1980`) run with the bit-exact ports in `unbanished-game`, or a save's stored map.
//! The drawing side (`view::world`) builds its mesh and textures on top of it.

use unbanished_formats::game::{
    environment_description, terrain_generator, EnvironmentDescription, TerrainGenerator,
};
use unbanished_formats::pkg::Archive;
use unbanished_formats::Result;
use unbanished_game::terrain::{Generator, Heights, SizeRecord, WaterMesh};

use super::world_params::WorldParams;

/// A generated or saved map: everything the simulation and the save need, with no mesh or
/// textures.
pub struct Map {
    /// The terrain profile (`Game/Terrain/*.rsc`), read for its materials.
    pub profile: TerrainGenerator,
    /// The terrain material: the profile's first, or the one a save stored.
    pub material: String,
    pub heights: Heights,
    /// `size²` texels of `[dirt, rock, light, snow]` bytes (BGRA8 in memory).
    pub surface: Vec<u8>,
    pub environment: EnvironmentDescription,
    /// MapComponent `+0x1e0`: the river centre lines in map coordinates. `FUN_1400b1980` fills
    /// list `i` with the positions of generator path `i`'s 28-byte records, last to first
    /// (verified against the saved paths of a generated map).
    pub river_paths: Vec<Vec<[f32; 2]>>,
    /// The rivers' and lakes' surface (`FUN_1400b0730`).
    pub water: WaterMesh,
}

/// A resource reference with its object name (`:resource` when it has none).
pub(crate) fn with_object(reference: &str) -> String {
    let path = reference.replace('\\', "/");
    if path.contains(':') {
        path
    } else {
        format!("{path}:resource")
    }
}

fn environment(archive: &Archive) -> Result<EnvironmentDescription> {
    environment_description(&archive.read_ref("Environment/Environment.rsc:resource")?)
}

impl Map {
    /// Generate the map New Game makes for these settings.
    pub fn generate(archive: &Archive, params: &WorldParams) -> Result<Map> {
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
        let water = unbanished_game::terrain::water_mesh(&g.paths, &g.lakes, &heights, size);
        let material = profile.materials[0].clone();
        Ok(Map {
            profile,
            material,
            heights: Heights {
                size,
                values: heights,
            },
            surface,
            environment: environment(archive)?,
            river_paths,
            water,
        })
    }

    /// A saved game's map: its stored terrain, surface, rivers and water instead of a
    /// generated one (`FUN_1400b59a0` restores them; nothing is regenerated on load).
    pub fn from_save(archive: &Archive, saved: &crate::saves::saved::SavedGame) -> Result<Map> {
        let map = saved
            .state
            .terrain_map
            .clone()
            .ok_or_else(|| unbanished_formats::Error::Format("save has no terrain".into()))?;
        let terrain_ref = saved.reference(saved.state.terrain).unwrap_or_default();
        // A mod's terrain profile may be missing: the saved map carries its own materials, so
        // the base Valleys profile stands in (the profile is only read for its materials).
        let profile = match archive.read_ref(&with_object(terrain_ref)) {
            Ok(data) => terrain_generator(&data)?,
            Err(e) => {
                log::warn!("{terrain_ref}: {e}; using Valleys");
                terrain_generator(&archive.read_ref("Game/Terrain/Valleys.rsc:resource")?)?
            }
        };
        let material = map
            .terrain_material
            .clone()
            .unwrap_or_else(|| profile.materials[0].clone());
        Ok(Map {
            profile,
            material,
            heights: Heights {
                size: map.dims[0] as usize,
                values: map.heights,
            },
            surface: map.surface,
            environment: environment(archive)?,
            river_paths: map.river_paths,
            water: WaterMesh {
                vertices: map.water_vertices,
                indices: map.water_indices,
                rivers: map.river_ranges,
                lakes: map.lake_ranges,
            },
        })
    }
}
