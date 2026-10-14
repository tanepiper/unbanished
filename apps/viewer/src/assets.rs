//! Loading game data from the player's own `.pkg` files and indexing it by resource class.

use std::rc::Rc;

use banished_formats::crc::type_id;
use banished_formats::pkg::{Archive, Package};
use banished_formats::types::{self, GraphicsMesh, ImageBuffer, MaterialInstance, SoundBuffer};
use banished_formats::{crs, Result};

pub struct Assets {
    pub archive: Rc<Archive>,
    /// Entry names per browsable class, sorted.
    pub meshes: Vec<String>,
    pub textures: Vec<String>,
    pub sounds: Vec<String>,
    pub dialogs: Vec<String>,
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
        let (mesh, image, sound, dialog) = (
            type_id("GraphicsMesh"),
            type_id("ImageBuffer"),
            type_id("SoundBuffer"),
            type_id("Dialog"),
        );
        let mut assets = Assets {
            archive: Rc::new(Archive::default()),
            meshes: Vec::new(),
            textures: Vec::new(),
            sounds: Vec::new(),
            dialogs: Vec::new(),
        };
        for name in archive.names() {
            if !name.ends_with(".crs") {
                continue;
            }
            let Ok(header) = archive.read(name).and_then(|d| crs::header(&d)) else {
                continue;
            };
            let list = match header.type_id {
                t if t == mesh => &mut assets.meshes,
                t if t == image => &mut assets.textures,
                t if t == sound => &mut assets.sounds,
                t if t == dialog => &mut assets.dialogs,
                _ => continue,
            };
            list.push(name.to_string());
        }
        log::info!(
            "{} resources: {} meshes, {} textures, {} sounds, {} dialogs",
            archive.len(),
            assets.meshes.len(),
            assets.textures.len(),
            assets.sounds.len(),
            assets.dialogs.len()
        );
        assets.archive = Rc::new(archive);
        Ok(assets)
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

    pub fn image_entry(&self, name: &str) -> Result<ImageBuffer> {
        types::image_buffer(&self.archive.read(name)?)
    }

    pub fn sound(&self, name: &str) -> Result<SoundBuffer> {
        types::sound_buffer(&self.archive.read(name)?)
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
