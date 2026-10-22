//! Starting a removal, road, bridge or area tool from its description, and the cells each one covers.

use super::*;

impl RemovalTool {
    /// Starts the drag tool for a template (removal, road, bridge, area or fence).
    ///
    /// Original: `FUN_14010a190`, `FUN_14010a0a0`.
    pub fn start(archive: &Archive, town: &mut StartingTown, reference: &str) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let Some(data) = read(archive, &format!("{file}:createremoval")) else {
            return Self::start_road(archive, town, reference);
        };
        let desc = create_removal_description(&data)
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()?;
        let marker = desc
            .template
            .as_deref()
            .and_then(|t| town.load_template(archive, t));
        let drag = desc.template.as_deref().and_then(|t| {
            let file = t.split(':').next()?.replace('\\', "/");
            create_drag_description(&read(archive, &format!("{file}:createdrag"))?).ok()
        });
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        // Remove Roads' markers are placed by the town after the simulation decides them.
        if desc.roads {
            if let Some(t) = marker {
                town.site_template(archive, t);
            }
        }
        Some(RemovalTool {
            reference: reference.to_string(),
            desc,
            marker,
            drag,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: None,
            bridge: None,
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        })
    }

    /// RoadTool's start (`FUN_14010abb0`): the template's CreateRoadDescription.
    pub(super) fn start_road(
        archive: &Archive,
        town: &mut StartingTown,
        reference: &str,
    ) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let Some(data) = read(archive, &format!("{file}:createroad")) else {
            return Self::start_bridge(archive, town, reference);
        };
        let d = create_road_description(&data)
            .map_err(|e| log::warn!("{reference}: {e}"))
            .ok()?;
        let template = town.load_template(archive, reference)?;
        let ty = town.templates[template]
            .map
            .as_ref()
            .map_or(0, |m| m.cell_type as u8);
        let material = d
            .material
            .as_deref()
            .and_then(|m| crate::town::material_index(archive, &mut town.materials, m));
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        Some(RemovalTool {
            reference: reference.to_string(),
            desc: CreateRemovalDescription::default(),
            marker: None,
            drag: None,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: Some(RoadSpec {
                kind: d.kind,
                ty,
                material,
            }),
            bridge: None,
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        })
    }

    /// BridgeTool's start (`FUN_140100c70`): the template's CreateBridgeDescription.
    pub(super) fn start_bridge(
        archive: &Archive,
        town: &mut StartingTown,
        reference: &str,
    ) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let Some(bridge) = town.bridge_spec(archive, reference) else {
            return Self::start_area(archive, town, reference);
        };
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        Some(RemovalTool {
            reference: reference.to_string(),
            desc: CreateRemovalDescription::default(),
            marker: None,
            drag: None,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: None,
            bridge: Some(bridge),
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        })
    }

    /// DragPlacementTool's start (`FUN_1401018b0`): the template's CreateDragDescription and
    /// MapDescription; or FencedTool's (`FUN_1400e2220`) for a CreateFencedDescription.
    pub(super) fn start_area(
        archive: &Archive,
        town: &mut StartingTown,
        reference: &str,
    ) -> Option<Self> {
        let file = reference.split(':').next()?.replace('\\', "/");
        let toolbar =
            read(archive, &format!("{file}:toolbar")).and_then(|d| toolbar_description(&d).ok());
        let template = town.load_template(archive, reference)?;
        let mut tool = RemovalTool {
            reference: reference.to_string(),
            desc: CreateRemovalDescription::default(),
            marker: None,
            drag: None,
            toolbar,
            dragging: false,
            waiting: false,
            start: -1,
            end: -1,
            sounded: -1,
            held: false,
            road: None,
            bridge: None,
            area: None,
            fenced: None,
            rotation: 0,
            restrict: None,
            manhattan: true,
        };
        if let Some(data) = read(archive, &format!("{file}:createdrag")) {
            let drag = create_drag_description(&data)
                .map_err(|e| log::warn!("{reference}: {e}"))
                .ok()?;
            let clear = drag
                .reference
                .as_deref()
                .and_then(|c| town.load_template(archive, c));
            tool.area = Some(AreaSpec {
                template,
                drag,
                clear,
            });
            return Some(tool);
        }
        let d = unbanished_formats::game::create_fenced_description(&read(
            archive,
            &format!("{file}:createfenced"),
        )?)
        .map_err(|e| log::warn!("{reference}: {e}"))
        .ok()?;
        let clear = d
            .clear
            .as_deref()
            .and_then(|c| town.load_template(archive, c));
        tool.fenced = Some((d, clear));
        Some(tool)
    }

    /// DragPlacementTool's check (slot `+0x70`, `FUN_140100fb0`) of the rectangle from `a` to
    /// `b`: 1 smaller than `+0x30 × +0x34`, 2 larger than `+0x28 × +0x2c`, 3 a cell that does
    /// not admit it (`FUN_1400a4aa0(mask +0x20, 0, 100, 0, +0x24, the clear template +0x38)`),
    /// else 0.
    pub(super) fn area_check(
        town: &StartingTown,
        spec: &AreaSpec,
        terrain: &HeightGrid,
        a: i32,
        b: i32,
    ) -> u8 {
        let n = town.cells.size as i32;
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let d = &spec.drag;
        if w < d.value_30 || h < d.value_34 {
            return 1;
        }
        if d.value_28 < w || d.value_2c < h {
            return 2;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                if !Self::area_cell(town, spec, terrain, x, y) {
                    return 3;
                }
            }
        }
        0
    }

    pub(super) fn area_cell(
        town: &StartingTown,
        spec: &AreaSpec,
        terrain: &HeightGrid,
        x: i32,
        y: i32,
    ) -> bool {
        cell_admits(
            &town.cells,
            terrain,
            x,
            y,
            spec.drag.mask,
            0.0,
            100.0,
            0.0,
            spec.drag.override_occupant,
            false,
            |o| Some(town.placed[o].template) == spec.clear,
        )
    }

    /// `FUN_140100290`: the bridge's cells from the drag's start to its end on the same row or
    /// column, valid (green) where the check holds and the cell is of a spanned or shown type
    /// with no one on it, or, before the drag has length, a bank.
    pub(super) fn bridge_cells(
        town: &StartingTown,
        spec: &crate::town::BridgeSpec,
        a: i32,
        b: i32,
    ) -> Vec<(i32, i32, bool)> {
        let n = town.cells.size as i32;
        let b = if b < 0 { a } else { town.bridge_end(a, b) };
        let (ok, _) = town.bridge_check(spec, a, b);
        let d = &spec.desc;
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = (y * n + x) as usize;
                let t = 1u32 << (town.cells.types[i].0 & 0x1f);
                let valid =
                    (ok && (d.span | d.shown) & t != 0 && town.cells.occupants[i].is_none())
                        || (a == b && d.banks & t != 0);
                out.push((x, y, valid));
            }
        }
        out
    }

    /// `FUN_14010a4b0` / the start of `FUN_14010a200`: a road's cells between the drag's ends
    /// (sorted), the ends neither water, high nor rough ground nor occupied. The path manager
    /// steps without diagonals, with the Manhattan heuristic unless key 0x16 is held
    /// (`FUN_14010ab60`).
    /// The check and the draw (`FUN_14010a4b0`, `FUN_14010a660`) let the path cross the ends'
    /// own types too (`ends | 0xa17`); the apply (`FUN_14010a200`) only `0xa17`.
    pub(super) fn road_cells(&self, town: &mut StartingTown, a: i32, b: i32) -> Option<Vec<i32>> {
        self.road_path(town, a, b, true)
    }

    pub(super) fn road_path(
        &self,
        town: &mut StartingTown,
        a: i32,
        b: i32,
        with_ends: bool,
    ) -> Option<Vec<i32>> {
        let (a, b) = if b < a { (b, a) } else { (a, b) };
        if a < 0 {
            return None;
        }
        let (ia, ib) = (a as usize, b as usize);
        let ends =
            (1u32 << (town.cells.types[ia].0 & 0x1f)) | (1u32 << (town.cells.types[ib].0 & 0x1f));
        if ends & ROAD_ENDS != 0
            || town.cells.occupants[ia].is_some()
            || town.cells.occupants[ib].is_some()
        {
            return None;
        }
        let mask = if with_ends {
            ends | ROAD_MASK
        } else {
            ROAD_MASK
        };
        town.agents
            .road_path(&town.cells, a, b, mask, self.manhattan)
            .filter(|p| !p.is_empty())
    }
}
