//! Placing, building and removing things: placed and fenced structures, roads, bridges, floors, terraforming, building, destroying and clearing.

mod build;
pub use build::{
    build_description, clear_area_description, destroy_description, BuildDescription,
    ClearAreaDescription, DestroyDescription,
};

use super::*;

/// `CreatePlacedDescription`: how a building is placed; the start structures take their size from
/// it.
///
/// Original: `FUN_14011eda0`, `FUN_1400a7490`.
#[derive(Debug, Clone)]
pub struct CreatePlacedDescription {
    // +0x20
    /// Not identified yet.
    pub flags: u32,
    // +0x30
    /// Not identified yet.
    pub values: Vec<u32>,
    // +0x28
    /// Not identified yet.
    pub name: String,
    // +0x44, +0x48
    /// The footprint's width in cells.
    pub width: i32,
    /// The footprint's height in cells.
    pub height: i32,
    // +0x40
    /// Not identified yet.
    pub value: i32,
    // +0x50
    /// Not identified yet.
    pub related: Option<String>,
}

/// Read a [`CreatePlacedDescription`] from the bytes of its resource.
pub fn create_placed_description(data: &[u8]) -> Result<CreatePlacedDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreatePlacedDescription") {
            return format_err(format!(
                "expected CreatePlacedDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let flags = r.u32()?;
            let values = (0..r.count()?).map(|_| r.u32()).collect::<Result<_>>()?;
            Ok(CreatePlacedDescription {
                flags,
                values,
                name: r.string()?,
                width: r.i32()?,
                height: r.i32()?,
                value: r.i32()?,
                related: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

// vtable 0x140166a10; +0x20; +0x28..+0x50; +0x58; +0x5c; +0x60; +0x64; +0x68; +0x70
/// `CreateFencedDescription`: the fenced tool's (FencedTool) cell-type mask, its six part templates
/// (shelter, end, corner, straight, long straight, gate), the largest, and smallest, size, the
/// shelter centred on its side, and the template whose entities may stand in the way.
///
/// Original: `FUN_140048250`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateFencedDescription {
    /// The cell types it may cover.
    pub mask: u32,
    /// The part templates: shelter, end, corner, straight, long straight, gate.
    pub parts: Vec<Option<String>>,
    /// The largest size.
    pub max: (i32, i32),
    /// The smallest size.
    pub min: (i32, i32),
    /// Whether the shelter is centred on its side.
    pub centered: bool,
    /// The template whose entities may stand in the way.
    pub clear: Option<String>,
}

/// Read a [`CreateFencedDescription`] from the bytes of its resource.
pub fn create_fenced_description(data: &[u8]) -> Result<CreateFencedDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateFencedDescription") {
            return format_err(format!(
                "expected CreateFencedDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let mask = r.u32()?;
            let parts = (0..6).map(|_| r.external_ref()).collect::<Result<_>>()?;
            Ok(CreateFencedDescription {
                mask,
                parts,
                max: (r.i32()?, r.i32()?),
                min: (r.i32()?, r.i32()?),
                centered: r.bool()?,
                clear: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28
/// `CreateRoadDescription`: the road kind (0 stone, 1 dirt; a lower kind replaces a higher), the
/// raw material a cell takes (none for dirt).
///
/// Original: `FUN_14011f150`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateRoadDescription {
    /// The road kind: 0 stone, 1 dirt. A lower kind replaces a higher one.
    pub kind: u8,
    /// The raw material a cell takes (none for dirt).
    pub material: Option<String>,
}

/// Read a [`CreateRoadDescription`] from the bytes of its resource.
pub fn create_road_description(data: &[u8]) -> Result<CreateRoadDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateRoadDescription") {
            return format_err(format!(
                "expected CreateRoadDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(CreateRoadDescription {
                kind: r.u8()?,
                material: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}

// vtable 0x140167f20; +0x20; +0x24; +0x28; +0x2c; +0x30; +0x34; +0x38
/// `CreateBridgeDescription`: the bridge tool's (BridgeTool) cell-type masks: the cells it spans
/// (water), its banks, further cells drawn as valid, at least one spanned cell of these, the cells
/// beside it within (its half width); the part templates (start, 1, 2, 4 and 8 cells long).
///
/// Original: `FUN_14011eaa0`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateBridgeDescription {
    /// The cell types it spans (water).
    pub span: u32,
    /// The cell types of its banks.
    pub banks: u32,
    /// Further cell types drawn as valid.
    pub shown: u32,
    /// The cell types of which at least one spanned cell is needed.
    pub needs: u32,
    /// The cell types allowed beside it.
    pub sides: u32,
    /// Its half width, in cells.
    pub half_width: i32,
    /// The part templates: the start, then 1, 2, 4 and 8 cells long.
    pub parts: Vec<Option<String>>,
}

/// Read a [`CreateBridgeDescription`] from the bytes of its resource.
pub fn create_bridge_description(data: &[u8]) -> Result<CreateBridgeDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateBridgeDescription") {
            return format_err(format!(
                "expected CreateBridgeDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let span = r.u32()?;
            let shown = r.u32()?;
            let needs = r.u32()?;
            let banks = r.u32()?;
            let sides = r.u32()?;
            let half_width = r.i32()?;
            let parts = (0..5).map(|_| r.external_ref()).collect::<Result<_>>()?;
            Ok(CreateBridgeDescription {
                span,
                banks,
                shown,
                needs,
                sides,
                half_width,
                parts,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28
/// `FloorDescription`: the picking mesh walked on, added on activation.
///
/// Original: `FUN_140124b50`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FloorDescription {
    /// The picking mesh walked on.
    pub mesh: Option<String>,
    /// Whether the floor is added on activation.
    pub active: bool,
}

/// Read a [`FloorDescription`] from the bytes of its resource.
pub fn floor_description(data: &[u8]) -> Result<FloorDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("FloorDescription") {
            return format_err(format!(
                "expected FloorDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(FloorDescription {
                mesh: r.external_ref()?,
                active: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20; +0x28; +0x30; +0x34
/// `TerraformDescription`: a tunnel's floor and ceiling picking meshes, each with the cell types
/// its vertices may change.
///
/// Original: `FUN_140058000`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerraformDescription {
    /// The floor's picking mesh.
    pub floor: Option<String>,
    /// The cell types the floor's vertices may change.
    pub floor_mask: u32,
    /// The ceiling's picking mesh.
    pub ceiling: Option<String>,
    /// The cell types the ceiling's vertices may change.
    pub ceiling_mask: u32,
}

/// Read a [`TerraformDescription`] from the bytes of its resource.
pub fn terraform_description(data: &[u8]) -> Result<TerraformDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("TerraformDescription") {
            return format_err(format!(
                "expected TerraformDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(TerraformDescription {
                floor: r.external_ref()?,
                floor_mask: r.u32()?,
                ceiling: r.external_ref()?,
                ceiling_mask: r.u32()?,
            })
        })
    })?;
    Ok(value)
}

/// `CreateRemovalDescription` (`Template\ClearX.rsc:createremoval`): what a RemovalTool drag does
/// to each cell.
///
/// Original: `FUN_14011ef90`, `FUN_140108ab0`.
#[derive(Debug, Clone, Default)]
pub struct CreateRemovalDescription {
    // +0x20
    /// Mark natural resources for removal.
    pub resources: bool,
    // +0x28/+0x30
    /// The resource templates it marks (empty: every one).
    pub filters: Vec<String>,
    // +0x38
    /// The marker entity's template (`Template\Clear.rsc:resource`).
    pub template: Option<String>,
    // +0x40; +0x41; +0x42; +0x43
    /// Remove roads.
    pub roads: bool,
    /// Demolish buildings.
    pub buildings: bool,
    /// Cancel.
    pub cancel: bool,
    /// Prioritise.
    pub priority: bool,
}

/// Read a [`CreateRemovalDescription`] from the bytes of its resource.
pub fn create_removal_description(data: &[u8]) -> Result<CreateRemovalDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateRemovalDescription") {
            return format_err(format!(
                "expected CreateRemovalDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let resources = r.bool()?;
            let n = r.count()?;
            let filters = (0..n)
                .map(|_| Ok(r.external_ref()?.unwrap_or_default()))
                .collect::<Result<_>>()?;
            Ok(CreateRemovalDescription {
                resources,
                filters,
                template: r.external_ref()?,
                roads: r.bool()?,
                buildings: r.bool()?,
                cancel: r.bool()?,
                priority: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// `CreateDragDescription` (`Template\X.rsc:createdrag`).
///
/// Original: `FUN_14011ec20`.
#[derive(Debug, Clone, Default)]
pub struct CreateDragDescription {
    // +0x20
    /// The cell types the drag may cover, as a mask of type bits.
    ///
    /// Original: read by `FUN_1400a4aa0`.
    pub mask: u32,
    /// Not identified yet.
    pub value_28: i32,
    /// Not identified yet.
    pub value_2c: i32,
    /// Not identified yet.
    pub value_30: i32,
    /// Not identified yet.
    pub value_34: i32,
    // +0x24
    /// Ignore cell occupants.
    pub override_occupant: bool,
    // +0x38
    /// An external reference.
    pub reference: Option<String>,
}

/// Read a [`CreateDragDescription`] from the bytes of its resource.
pub fn create_drag_description(data: &[u8]) -> Result<CreateDragDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("CreateDragDescription") {
            return format_err(format!(
                "expected CreateDragDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(CreateDragDescription {
                mask: r.u32()?,
                value_28: r.i32()?,
                value_2c: r.i32()?,
                value_30: r.i32()?,
                value_34: r.i32()?,
                override_occupant: r.bool()?,
                reference: r.external_ref()?,
            })
        })
    })?;
    Ok(value)
}
