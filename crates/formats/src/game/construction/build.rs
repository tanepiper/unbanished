//! Build, destroy and clear-area descriptions: the work of putting up and taking down.

use super::*;

/// `BuildDescription`: what constructing a building costs.
///
/// Original: `FUN_14010e610`.
#[derive(Debug, Clone)]
pub struct BuildDescription {
    // +0x20
    /// `(count, raw material)` requirements.
    pub materials: Vec<(i32, Option<String>)>,
    // +0x30
    /// Construction work.
    pub work: i32,
    // +0x34
    /// Shows `labelBuildMulti` in the CreateBar (costs per area).
    pub multi: bool,
}

/// Read a [`BuildDescription`] from the bytes of its resource.
pub fn build_description(data: &[u8]) -> Result<BuildDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("BuildDescription") {
            return format_err(format!(
                "expected BuildDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let materials = (0..r.count()?)
                .map(|_| Ok((r.i32()?, r.external_ref()?)))
                .collect::<Result<_>>()?;
            Ok(BuildDescription {
                materials,
                work: r.i32()?,
                multi: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28; +0x2c; +0x30
/// `DestroyDescription`: the template left in the building's place once it is torn down (the
/// quarry's and mine's hole; none for most), the ModelComponent's material set while damaged, the
/// decal while damaged, the lowest construction mesh a collapse steps down to.
///
/// Original: `FUN_140123340`, `FUN_14012a4f0`, `FUN_140122aa0`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DestroyDescription {
    /// The template left in the building's place once it is torn down (the quarry's and mine's hole; none for most).
    pub replace: Option<String>,
    /// The ModelComponent's material set while damaged.
    pub material: i32,
    /// The decal while damaged.
    pub decal: i32,
    /// The lowest construction mesh a collapse steps down to.
    pub min_mesh: i32,
}

/// Read a [`DestroyDescription`] from the bytes of its resource.
pub fn destroy_description(data: &[u8]) -> Result<DestroyDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("DestroyDescription") {
            return format_err(format!(
                "expected DestroyDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(DestroyDescription {
                replace: r.external_ref()?,
                material: r.i32()?,
                decal: r.i32()?,
                min_mesh: r.i32()?,
            })
        })
    })?;
    Ok(value)
}

/// `ClearAreaDescription` (`Template\X.rsc:cleararea`).
///
/// Original: `FUN_14011d5c0`.
#[derive(Debug, Clone, Default)]
pub struct ClearAreaDescription {
    // +0x20
    /// An external reference handed to the ClearArea work.
    ///
    /// Original: `FUN_1400ebe30`.
    pub reference: Option<String>,
    // +0x28..+0x2a
    /// The ClearArea work clears natural resources.
    pub clear_resources: bool,
    /// The ClearArea work clears empty cells.
    pub clear_cells: bool,
    /// The ClearArea work clears piles.
    pub clear_piles: bool,
    // +0x2b
    /// Register the work keeping its priority.
    pub keep_priority: bool,
    // +0x2c
    /// Stamp the footprint once cleared.
    ///
    /// Original: `FUN_14011d740`.
    pub stamp: bool,
    // +0x2d
    /// Remove everything in the footprint at once.
    ///
    /// Original: `FUN_14011d230`.
    pub instant: bool,
    /// Not identified yet.
    pub value_30: i32,
}

/// Read a [`ClearAreaDescription`] from the bytes of its resource.
pub fn clear_area_description(data: &[u8]) -> Result<ClearAreaDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("ClearAreaDescription") {
            return format_err(format!(
                "expected ClearAreaDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(ClearAreaDescription {
                reference: r.external_ref()?,
                clear_resources: r.bool()?,
                clear_cells: r.bool()?,
                clear_piles: r.bool()?,
                keep_priority: r.bool()?,
                stamp: r.bool()?,
                instant: r.bool()?,
                value_30: r.i32()?,
            })
        })
    })?;
    Ok(value)
}
