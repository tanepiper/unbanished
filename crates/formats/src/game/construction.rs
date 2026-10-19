//! Placing, building and removing things: placed and fenced structures, roads, bridges, floors, terraforming, building, destroying and clearing.

use super::*;

/// `CreatePlacedDescription` (serializer `FUN_14011eda0`): how a building is placed; the start
/// structures (`FUN_1400a7490`) take their size from it.
#[derive(Debug, Clone)]
pub struct CreatePlacedDescription {
    /// `+0x20`.
    pub flags: u32,
    /// `+0x30`.
    pub values: Vec<u32>,
    /// `+0x28`.
    pub name: String,
    /// `+0x44`, `+0x48`: footprint width and height in cells.
    pub width: i32,
    pub height: i32,
    /// `+0x40`.
    pub value: i32,
    /// `+0x50`.
    pub related: Option<String>,
}

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

/// `CreateFencedDescription` (serializer `FUN_140048250`): the fenced tool's (FencedTool,
/// vtable `0x140166a10`) cell-type mask `+0x20`, its six part templates `+0x28..+0x50` (shelter,
/// end, corner, straight, long straight, gate), the largest `+0x58`, `+0x5c` and smallest
/// `+0x60`, `+0x64` size, the shelter centred on its side `+0x68`, and the template whose
/// entities may stand in the way `+0x70`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateFencedDescription {
    pub mask: u32,
    pub parts: Vec<Option<String>>,
    pub max: (i32, i32),
    pub min: (i32, i32),
    pub centered: bool,
    pub clear: Option<String>,
}

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

/// `CreateRoadDescription` (serializer `FUN_14011f150`): `+0x20` the road kind (0 stone, 1 dirt;
/// a lower kind replaces a higher), `+0x28` the raw material a cell takes (none for dirt).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateRoadDescription {
    pub kind: u8,
    pub material: Option<String>,
}

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

/// `CreateBridgeDescription` (serializer `FUN_14011eaa0`): the bridge tool's (BridgeTool, vtable
/// `0x140167f20`) cell-type masks: `+0x20` the cells it spans (water), `+0x24` its banks, `+0x28`
/// further cells drawn as valid, `+0x2c` at least one spanned cell of these, `+0x30` the cells
/// beside it within `+0x34` (its half width); `+0x38` the part templates (start, 1, 2, 4 and 8
/// cells long).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateBridgeDescription {
    pub span: u32,
    pub banks: u32,
    pub shown: u32,
    pub needs: u32,
    pub sides: u32,
    pub half_width: i32,
    pub parts: Vec<Option<String>>,
}

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

/// `FloorDescription` (serializer `FUN_140124b50`): `+0x20` the picking mesh walked on, `+0x28`
/// added on activation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FloorDescription {
    pub mesh: Option<String>,
    pub active: bool,
}

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

/// `TerraformDescription` (serializer `FUN_140058000`): a tunnel's `+0x20` floor and `+0x28`
/// ceiling picking meshes, each with the cell types `+0x30`, `+0x34` its vertices may change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerraformDescription {
    pub floor: Option<String>,
    pub floor_mask: u32,
    pub ceiling: Option<String>,
    pub ceiling_mask: u32,
}

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

/// `CreateRemovalDescription` (`Template\ClearX.rsc:createremoval`, serializer
/// `FUN_14011ef90`): what a RemovalTool drag does to each cell (`FUN_140108ab0`).
#[derive(Debug, Clone, Default)]
pub struct CreateRemovalDescription {
    /// `+0x20`: mark natural resources for removal.
    pub resources: bool,
    /// `+0x28/+0x30`: the resource templates it marks (empty: every one).
    pub filters: Vec<String>,
    /// `+0x38`: the marker entity's template (`Template\Clear.rsc:resource`).
    pub template: Option<String>,
    /// `+0x40` roads, `+0x41` demolish buildings, `+0x42` cancel, `+0x43` prioritise.
    pub roads: bool,
    pub buildings: bool,
    pub cancel: bool,
    pub priority: bool,
}

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

/// `CreateDragDescription` (`Template\X.rsc:createdrag`, serializer `FUN_14011ec20`).
#[derive(Debug, Clone, Default)]
pub struct CreateDragDescription {
    /// `+0x20`: admitted cell types (`FUN_1400a4aa0`'s mask).
    pub mask: u32,
    pub value_28: i32,
    pub value_2c: i32,
    pub value_30: i32,
    pub value_34: i32,
    /// `+0x24`: ignore cell occupants.
    pub override_occupant: bool,
    /// `+0x38`: an external reference.
    pub reference: Option<String>,
}

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

/// `BuildDescription` (serializer `FUN_14010e610`): what constructing a building costs.
#[derive(Debug, Clone)]
pub struct BuildDescription {
    /// `+0x20`: `(count, raw material)` requirements.
    pub materials: Vec<(i32, Option<String>)>,
    /// `+0x30`: construction work.
    pub work: i32,
    /// `+0x34`: shows `labelBuildMulti` in the CreateBar (costs per area).
    pub multi: bool,
}

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

/// `DestroyDescription` (serializer `FUN_140123340`): `+0x20` the template left in the
/// building's place once it is torn down (the quarry's and mine's hole; none for most), `+0x28`
/// the ModelComponent's material set while damaged (`FUN_14012a4f0`), `+0x2c` the decal while
/// damaged, `+0x30` the lowest construction mesh a collapse steps down to (`FUN_140122aa0`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DestroyDescription {
    pub replace: Option<String>,
    pub material: i32,
    pub decal: i32,
    pub min_mesh: i32,
}

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

/// `ClearAreaDescription` (`Template\X.rsc:cleararea`, serializer `FUN_14011d5c0`).
#[derive(Debug, Clone, Default)]
pub struct ClearAreaDescription {
    /// `+0x20`: an external reference handed to the ClearArea work (`FUN_1400ebe30`).
    pub reference: Option<String>,
    /// `+0x28..+0x2a`: the ClearArea work's flags (natural resources, empty cells, piles).
    pub clear_resources: bool,
    pub clear_cells: bool,
    pub clear_piles: bool,
    /// `+0x2b`: register the work keeping its priority.
    pub keep_priority: bool,
    /// `+0x2c`: stamp the footprint once cleared (`FUN_14011d740`).
    pub stamp: bool,
    /// `+0x2d`: remove everything in the footprint at once (`FUN_14011d230`).
    pub instant: bool,
    pub value_30: i32,
}

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
