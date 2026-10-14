//! Loading game data from the player's own `.pkg` files.

use banished_formats::crc::type_id;
use banished_formats::pkg::{Archive, Package};
use banished_formats::types::{self, GraphicsMesh, ImageBuffer, MaterialInstance};
use banished_formats::{crs, Result};

pub struct Assets {
    pub archive: Archive,
    /// Entry names of every `GraphicsMesh` resource, sorted.
    pub meshes: Vec<String>,
}

impl Assets {
    pub fn from_packages(packages: Vec<(String, Vec<u8>)>) -> Result<Assets> {
        let mut archive = Archive::default();
        for (name, bytes) in packages {
            let pkg = Package::parse(bytes)?;
            log::info!(
                "{name}: {} entries, priority {}",
                pkg.entries.len(),
                pkg.header.priority
            );
            archive.add(pkg);
        }
        let mesh_id = type_id(GraphicsMesh::CLASS);
        let meshes = archive
            .names()
            .into_iter()
            .filter(|n| n.starts_with("Models\\") && n.ends_with(".crs"))
            .filter(|n| {
                archive
                    .read(n)
                    .ok()
                    .and_then(|d| crs::header(&d).ok())
                    .is_some_and(|h| h.type_id == mesh_id)
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        log::info!("{} resources, {} meshes", archive.len(), meshes.len());
        Ok(Assets { archive, meshes })
    }

    pub fn mesh(&self, name: &str) -> Result<GraphicsMesh> {
        types::graphics_mesh(&self.archive.read(name)?)
    }

    pub fn material_instance(&self, reference: &str) -> Result<MaterialInstance> {
        types::material_instance(&self.archive.read_ref(reference)?)
    }

    pub fn image(&self, reference: &str) -> Result<ImageBuffer> {
        types::image_buffer(&self.archive.read_ref(reference)?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_dir(dir: &std::path::Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pkg"))
        {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, std::fs::read(&path)?));
        }
    }
    Ok(out)
}
