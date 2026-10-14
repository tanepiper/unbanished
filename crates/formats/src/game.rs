//! Gameplay resources from the game executable's registrations.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// `TerrainGenerator` (serializer `FUN_1400b6b60`): a New Game terrain profile.
#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    pub string_table: Option<String>,
    pub name: String,
    pub flag: bool,
    /// Terrain, river and lake material instances.
    pub materials: [String; 3],
    /// Small / Medium / Large size records, 17 integers each in serialized (wire) order.
    pub sizes: Vec<[i32; 17]>,
}

pub fn terrain_generator(data: &[u8]) -> Result<TerrainGenerator> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("TerrainGenerator") {
            return format_err(format!(
                "expected TerrainGenerator, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let string_table = r.external_ref()?;
            let name = r.string()?;
            let flag = r.bool()?;
            let materials = [r.string()?, r.string()?, r.string()?];
            let mut sizes = Vec::new();
            for _ in 0..r.count()? {
                let mut rec = [0i32; 17];
                for v in &mut rec {
                    *v = r.i32()?;
                }
                sizes.push(rec);
            }
            Ok(TerrainGenerator {
                string_table,
                name,
                flag,
                materials,
                sizes,
            })
        })
    })?;
    Ok(value)
}
