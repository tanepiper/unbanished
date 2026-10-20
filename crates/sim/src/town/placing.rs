//! Placing structures on the map: areas, fenced pastures and their fence pieces.

use super::*;

/// A rectangle of cells (`x0, y0, x1, y1`, or `x, y, w, h`).
pub(super) type Cells4 = (i32, i32, i32, i32);

/// `FUN_1400e1460`: the shelter's cells `(x0, y0, x1, y1)` in the corner of the area the
/// rotation faces (or centred on a side), its turn (0, −90, 180, 90 degrees) and its centre
/// (the middle of its corner cells, at the area's mean ground height, `FUN_140106c00`).
pub(super) fn fenced_shelter(
    d: &unbanished_formats::game::CreateFencedDescription,
    size: (i32, i32),
    (x0, y0, x1, y1): Cells4,
    rotation: i32,
    ground: impl Fn(i32, i32) -> [f32; 3],
) -> Option<(Cells4, f32, [f32; 3])> {
    if x1 - x0 + 1 < d.min.0 || y1 - y0 + 1 < d.min.1 {
        return None;
    }
    let r = rotation.rem_euclid(4);
    let yaw = [0.0, -90.0, 180.0, 90.0][r as usize];
    let (sw, sh) = if r == 1 || r == 3 {
        (size.1, size.0)
    } else {
        size
    };
    let (mut a0, mut b0, mut a1, mut b1) = (x0, y0, x1, y1);
    if !d.centered {
        match r {
            0 => {
                a0 = a1 - sw + 1;
                b0 = b1 - sh + 1;
            }
            1 => {
                a0 = a1 - sw + 1;
                b1 = b0 - 1 + sh;
            }
            2 => {
                a1 = a0 - 1 + sw;
                b1 = b0 - 1 + sh;
            }
            _ => {
                b0 = b1 - sh + 1;
                a1 = a0 - 1 + sw;
            }
        }
    } else {
        match r {
            0 => {
                b1 = b0;
                a0 = ((a0 + a1) >> 1) - (sw >> 1);
                a1 = sw - 1 + a0;
            }
            1 => {
                a1 = a0;
                b0 = ((b0 + b1) >> 1) - (sh >> 1);
                b1 = sh - 1 + b0;
            }
            2 => {
                b0 = b1;
                a0 = ((a0 + a1) >> 1) - (sw >> 1);
                a1 = sw - 1 + a0;
            }
            _ => {
                a0 = a1;
                b0 = ((b0 + b1) >> 1) - (sh >> 1);
                b1 = sh - 1 + b0;
            }
        }
    }
    let mut mean = 0.0;
    let share = 1.0 / ((a1 - a0 + 1) * (b1 - b0 + 1)) as f32;
    for y in b0..=b1 {
        for x in a0..=a1 {
            mean += ground(x, y)[2] * share;
        }
    }
    let g0 = ground(a0, b0);
    let g1 = ground(a1, b1);
    let centre = [(g0[0] + g1[0]) * 0.5, (g0[1] + g1[1]) * 0.5, mean];
    Some(((a0, b0, a1, b1), yaw, centre))
}

/// `FUN_1400e0d90`: the pieces of one side from `from` to `to` (one cell wide) around the
/// shelter's cells `s`, as (part index, `x, y, w, h`, turn): an end piece on either side of
/// the shelter, single straight pieces where two will not fit, else long ones, the second of
/// which is the gate (none on a side the centred shelter crosses).
pub(super) fn fence_run(
    from: (i32, i32),
    to: (i32, i32),
    s: Cells4,
    turn: i32,
    centered: bool,
) -> Vec<(usize, Cells4, i32)> {
    let mut out = Vec::new();
    let (x0, mut y) = from;
    let (x1, y1) = to;
    let sx = i32::from(0 < x1 - x0);
    let sy = i32::from(0 < y1 - y);
    let (lw, lh) = if turn == 0 { (2, 1) } else { (1, 2) };
    let inside = |x: i32, y: i32| s.0 <= x && x <= s.2 && s.1 <= y && y <= s.3;
    let mut clear = true;
    if centered && y <= y1 {
        for cy in y..=y1 {
            for cx in x0..=x1 {
                if inside(cx, cy) {
                    clear = false;
                }
            }
        }
    }
    let (mut fence, mut past, mut long, mut gate) = (false, false, false, false);
    while y <= y1 {
        let mut x = x0;
        while x <= x1 {
            if inside(x, y) {
                fence = false;
                past = true;
                long = false;
            } else {
                let next = inside(x + sx, y + sy);
                if past && !fence {
                    out.push((1, (x, y, 1, 1), turn));
                } else if fence && next {
                    out.push((1, (x, y, 1, 1), if turn == 0 { 180 } else { -90 }));
                } else if x1 < x + sx * 2 || y1 < y + sy * 2 || next {
                    out.push((3, (x, y, 1, 1), turn));
                } else if gate || !long || !clear {
                    out.push((4, (x, y, lw, lh), turn));
                    x += sx;
                    y += sy;
                    long = true;
                } else {
                    out.push((5, (x, y, lw, lh), turn));
                    x += sx;
                    y += sy;
                    gate = true;
                }
                fence = true;
            }
            x += 1;
        }
        y += 1;
    }
    out
}

impl StartingTown {
    /// A dragged area (CreateDrag: fields, orchards, the storage yard) over `rect` (`x, y, w,
    /// h`): the entity, its zone over every cell, and its site (DragPlacementTool's apply,
    /// `FUN_140100dd0`, through `removal_tool`; tests also place areas directly).
    pub fn place_area(
        &mut self,
        archive: &Archive,
        reference: &str,
        rect: (i32, i32, i32, i32),
        terrain: &HeightGrid,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let (x, y, w, h) = rect;
        let n = self.cells.size as i32;
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (
            x as f32 + (w - 1) as f32 / 2.0 - half,
            y as f32 + (h - 1) as f32 / 2.0 - half,
        );
        let mut normal = [0.0; 3];
        let z = terrain.ground(px, py, &mut normal);
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (x, y),
            footprint: (w, h),
            position: [px, py, z],
            yaw: 0.0,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        for cy in y..y + h {
            for cx in x..x + w {
                self.cells.occupants[(cy * n + cx) as usize] = Some(id);
            }
        }
        self.add_site(archive, id);
        Some(id)
    }

    /// The fenced tool (FencedTool, vtable `0x140166a10`) dragged from cell `a` to cell `b`
    /// with the shelter turned `rotation` quarter turns: slot `+0x70` (`FUN_1400e0960`) checks
    /// the area, slot `+0x68` (`FUN_1400e02d0`) places the entity over it with its shelter,
    /// corners and fence runs (its MultiObjectComponent's parts). The drag runs in
    /// `removal_tool`'s fenced mode. Returns the entity.
    pub fn place_fenced(
        &mut self,
        archive: &Archive,
        reference: &str,
        a: (i32, i32),
        b: (i32, i32),
        rotation: i32,
        terrain: &HeightGrid,
    ) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let key = normalise(reference);
        let d = read(archive, &key.replace(":resource", ":createfenced"))
            .and_then(|d| unbanished_formats::game::create_fenced_description(&d).ok())?;
        let mut parts = Vec::with_capacity(6);
        for p in &d.parts {
            parts.push(p.as_deref().and_then(|p| self.load_template(archive, p)));
        }
        let clear = d
            .clear
            .as_deref()
            .and_then(|c| self.load_template(archive, c));
        let shelter = d.parts.first().cloned().flatten()?;
        let size = read(
            archive,
            &normalise(&shelter).replace(":resource", ":createplaced"),
        )
        .and_then(|d| unbanished_formats::game::create_placed_description(&d).ok())
        .map(|c| (c.width, c.height))?;
        let n = self.cells.size as i32;
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        // `FUN_1400e0960`: the size, then every cell: of an allowed type, no occupant but the
        // clear template's, no zone, and not too high.
        if w < d.min.0 || h < d.min.1 || d.max.0 < w || d.max.1 < h {
            return None;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                if !self.fenced_cell_ok(&d, clear, x, y, terrain) {
                    return None;
                }
            }
        }
        let (s, yaw, centre) = fenced_shelter(&d, size, (x0, y0, x1, y1), rotation, |x, y| {
            unbanished_game::town::floor_ground(
                &self.cells.floors,
                self.cells.size,
                terrain,
                y * n + x,
            )
        })?;
        let mut degrees = yaw as i32;
        while degrees < 0 {
            degrees += 360;
        }
        // `FUN_1400c8bf0` of the tool's template, its MapComponent over the area.
        let entity = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (x0, y0),
            footprint: (w, h),
            // `FUN_1400e02d0` sets only the MapComponent (its rotation the tool's): the model
            // stays at the origin, unturned (`Placed::model`).
            position: [0.0; 3],
            yaw: degrees as f32,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        // `FUN_140106da0`: the clear template's entities in the area go.
        if let Some(c) = clear {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(o) = self.cells.occupants[(y * n + x) as usize] {
                        if self.placed[o].template == c && !self.construction.destroy.contains(&o) {
                            self.construction.destroy.push(o);
                        }
                    }
                }
            }
        }
        let mut multi = Vec::new();
        // The shelter, turned with the tool.
        if let Some(t) = parts[0] {
            let rect = (s.0, s.1, s.2 - s.0 + 1, s.3 - s.1 + 1);
            multi.push(self.fence_part(t, rect, degrees, centre));
        }
        // `FUN_1400e1150` kind 2: a corner at each corner outside the shelter.
        let outside = |x: i32, y: i32| x < s.0 || s.2 < x || y < s.1 || s.3 < y;
        for (x, y, turn) in [(x0, y0, 90), (x0, y1, 0), (x1, y1, -90), (x1, y0, 180)] {
            if outside(x, y) {
                if let Some(p) = self.fence_piece(parts[2], (x, y, 1, 1), turn, terrain) {
                    multi.push(p);
                }
            }
        }
        // `FUN_1400e0d90`: the four sides between the corners.
        for (from, to, turn) in [
            ((x0 + 1, y0), (x1 - 1, y0), 0),
            ((x0 + 1, y1), (x1 - 1, y1), 0),
            ((x0, y0 + 1), (x0, y1 - 1), 90),
            ((x1, y0 + 1), (x1, y1 - 1), 90),
        ] {
            for (kind, rect, t) in fence_run(from, to, s, turn, d.centered) {
                if let Some(p) = self.fence_piece(parts[kind], rect, t, terrain) {
                    multi.push(p);
                }
            }
        }
        // `FUN_1400c9d70` of the entity last: its zone and its site.
        self.register_entity(entity);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                self.cells.occupants[(cy * n + cx) as usize] = Some(entity);
            }
        }
        self.add_site(archive, entity);
        if let Some(site) = self.construction.sites.get_mut(&entity) {
            site.parts = multi;
            // MultiObjectComponent `+0x58`: the border's cells.
            site.multi_stages = Some((x1 - x0 + 1 + y1 - y0 + 1) * 2 - 4);
        }
        Some(entity)
    }

    /// `FUN_1400e0960`'s test of one cell: on the map, of an allowed type, no occupant but the
    /// clear template's, no zone, and not too high.
    pub fn fenced_cell_ok(
        &self,
        d: &unbanished_formats::game::CreateFencedDescription,
        clear: Option<usize>,
        x: i32,
        y: i32,
        terrain: &HeightGrid,
    ) -> bool {
        let n = self.cells.size as i32;
        if x < 0 || n <= x || y < 0 || n <= y {
            return false;
        }
        let i = (y * n + x) as usize;
        if !d.mask & (1 << (self.cells.types[i].0 & 0x1f)) != 0 {
            return false;
        }
        if let Some(o) = self.cells.occupants[i] {
            if clear.is_none() || Some(self.placed[o].template) != clear {
                return false;
            }
        }
        if self.cells.flags[i] != 0 {
            return false;
        }
        // `FUN_1400a48b0(.., 0, normal)`: the ground or a floor, facing up, within 100.
        let half = (n >> 1) as f32 - 0.5;
        let (px, py) = (x as f32 - half, y as f32 - half);
        let mut normal = [0.0; 3];
        let z = self
            .cells
            .floors
            .ground_normal(self.cells.size, terrain, px, py, &mut normal);
        0.0 <= normal[2] && (z - 0.0).abs() <= 100.0
    }

    /// `FUN_1400e1150`: a part over `rect` (`x, y, w, h`) at the middle of its corner cells'
    /// ground, turned `degrees`.
    pub(super) fn fence_piece(
        &mut self,
        template: Option<usize>,
        rect: (i32, i32, i32, i32),
        degrees: i32,
        terrain: &HeightGrid,
    ) -> Option<usize> {
        let t = template?;
        let n = self.cells.size as i32;
        let (x, y, w, h) = rect;
        let g0 = unbanished_game::town::floor_ground(
            &self.cells.floors,
            self.cells.size,
            terrain,
            n * y + x,
        );
        let g1 = unbanished_game::town::floor_ground(
            &self.cells.floors,
            self.cells.size,
            terrain,
            n * (y + h - 1) + x + w - 1,
        );
        // The corners' midpoint; `FUN_1400e1150` then sets z to 0.
        let centre = [(g0[0] + g1[0]) * 0.5, (g0[1] + g1[1]) * 0.5, 0.0];
        let mut degrees = degrees;
        while degrees < 0 {
            degrees += 360;
        }
        Some(self.fence_part(t, rect, degrees, centre))
    }

    /// A MultiObject part (`FUN_1400c8bf0`, its MapComponent and model, `FUN_1400c9d70`): it
    /// stamps the map with its parent (entity event 1).
    pub(super) fn fence_part(
        &mut self,
        template: usize,
        rect: (i32, i32, i32, i32),
        degrees: i32,
        position: [f32; 3],
    ) -> usize {
        let t = &self.templates[template];
        let (variant, mesh) = t.model.as_ref().map_or((0, 0), |m| {
            (m.variant.max(0) as usize, m.mesh.max(0) as usize)
        });
        let id = self.placed.len();
        self.placed.push(Placed {
            template,
            bucket: t.bucket,
            cell: (rect.0, rect.1),
            footprint: (rect.2, rect.3),
            position,
            yaw: degrees as f32,
            variant,
            mesh,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        self.register_entity(id);
        id
    }

    /// The animals' context over the town.
    #[cfg(test)]
    pub fn with_wild<R>(
        &mut self,
        terrain: &HeightGrid,
        rng: &mut Random,
        f: impl FnOnce(&mut unbanished_game::nature::wildlife::Wild) -> R,
    ) -> R {
        let mut wild = unbanished_game::nature::wildlife::Wild {
            wildlife: &mut self.wildlife,
            placed: &mut self.placed,
            templates: &self.templates,
            cells: &self.cells,
            terrain,
            rng,
            scheduler: &mut self.scheduler,
        };
        f(&mut wild)
    }
}
