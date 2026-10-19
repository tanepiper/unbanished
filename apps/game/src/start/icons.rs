//! Interface data for the town: profession lookup, the minimap overlay, status and building icons.

use super::*;

impl StartingTown {
    /// `Game/Profession/Profession.rsc:<key>` (read on demand; presentation only).
    pub fn profession(&self, key: &str) -> Option<banished_formats::game::Profession> {
        self.archive_professions
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, p)| p.clone())
    }

    /// The minimap overlay (`Game +0x148`): every live entity whose MapDescription has the
    /// minimap flag (`+0x32`) fills its map rectangle with its colour (`FUN_140129680` →
    /// `FUN_1400aa200`, rows flipped so the top row is north). RGBA bytes, `size²` pixels.
    pub fn minimap_overlay(&self) -> Vec<u8> {
        let n = self.cells.size as i32;
        let mut out = vec![0u8; (n * n * 4) as usize];
        for p in self.placed.iter().filter(|p| p.alive) {
            let Some(map) = self.templates[p.template]
                .map
                .as_ref()
                .filter(|m| m.minimap)
            else {
                continue;
            };
            let c = map.color;
            let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8];
            let ((x, y), (w, h)) = (p.cell, p.footprint);
            for j in 0..h {
                for i in 0..w {
                    let (px, row) = (x + i, n - y - j - 1);
                    if (0..n).contains(&px) && (0..n).contains(&row) {
                        let o = ((row * n + px) * 4) as usize;
                        out[o..o + 4].copy_from_slice(&rgba);
                    }
                }
            }
        }
        out
    }

    /// StatusIconComponent billboards (`FUN_1401310a0` anchors): each building with status bits
    /// ([`StartingTown::building_icons`]), and each citizen's above its model (position + model
    /// box height).
    pub fn status_icons(&self) -> Vec<crate::gfx::status_icons::StatusIcon> {
        let mut icons = self.building_icons();
        let Some(d) = &self.citizen_icons else {
            return icons;
        };
        let Some(sheet) = d.sheet.clone() else {
            return icons;
        };
        icons.extend(
            self.citizens
                .iter()
                .zip(&self.agents.states)
                .zip(&self.agents.characters)
                .filter(|((c, s), _)| c.alive && s.status != 0)
                .map(|((c, s), ch)| crate::gfx::status_icons::StatusIcon {
                    anchor: glam::Vec3::from(ch.position)
                        + glam::Vec3::Z * self.citizen_heights[usize::from(!c.male)],
                    mask: s.status,
                    sheet: sheet.clone(),
                    size: d.size,
                    screen_constant: d.screen_constant,
                }),
        );
        icons
    }

    /// The icon sheets `status_icons` refers to.
    pub fn icon_sheet_refs(&self) -> Vec<String> {
        let mut refs: Vec<String> = self
            .citizen_icons
            .iter()
            .chain(
                self.construction
                    .templates
                    .iter()
                    .filter_map(|t| t.status_icon.as_ref()),
            )
            .filter_map(|d| d.sheet.clone())
            .collect();
        refs.sort();
        refs.dedup();
        refs
    }

    /// The buildings' StatusIconComponents with bits set (`FUN_1401310a0`): above the model
    /// (its position plus its height), or for a MultiObject (or without a model) the map
    /// rectangle's centre cell on the ground plus 1.
    pub(super) fn building_icons(&self) -> Vec<crate::gfx::status_icons::StatusIcon> {
        let mut out = Vec::new();
        for (&e, site) in &self.construction.sites {
            let t = &self.construction.templates[site.template];
            let Some(d) = &t.status_icon else { continue };
            let Some(sheet) = d.sheet.clone() else {
                continue;
            };
            let mut mask = self
                .construction
                .workforce
                .workplaces
                .get(&e)
                .map_or(0, |w| w.status);
            if let Some(h) = self.construction.homes.get(&e) {
                mask |= h.status;
            }
            if mask == 0 || !self.placed[e].alive {
                continue;
            }
            let p = &self.placed[e];
            let anchor = match t.icon_height.filter(|_| site.parts.is_empty()) {
                Some(h) => glam::Vec3::from(p.position) + glam::Vec3::Z * h,
                None => {
                    let (x, y) = p.cell;
                    let (w, h) = p.footprint;
                    let n = self.cells.size as i32;
                    let half = (n >> 1) as f32 - 0.5;
                    let (cx, cy) = ((x + (w >> 1)) as f32 - half, (y + (h >> 1)) as f32 - half);
                    glam::Vec3::new(cx, cy, p.position[2] + 1.0)
                }
            };
            out.push(crate::gfx::status_icons::StatusIcon {
                anchor,
                mask,
                sheet,
                size: d.size,
                screen_constant: d.screen_constant,
            });
        }
        out
    }
}
