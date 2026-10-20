//! The town's status and building icons.

use super::*;

impl TownView {
    /// StatusIconComponent billboards (`FUN_1401310a0` anchors): each building with status bits
    /// ([`StartingTown::building_icons`]), and each citizen's above its model (position + model
    /// box height).
    pub fn status_icons(&self, town: &StartingTown) -> Vec<crate::gfx::status_icons::StatusIcon> {
        let mut icons = self.building_icons(town);
        let Some(d) = &self.citizen_icons else {
            return icons;
        };
        let Some(sheet) = d.sheet.clone() else {
            return icons;
        };
        icons.extend(
            town.citizens
                .iter()
                .zip(&town.agents.states)
                .zip(&town.agents.characters)
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
    pub fn icon_sheet_refs(&self, town: &StartingTown) -> Vec<String> {
        let mut refs: Vec<String> = self
            .citizen_icons
            .iter()
            .chain(
                town.construction
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
    pub(super) fn building_icons(
        &self,
        town: &StartingTown,
    ) -> Vec<crate::gfx::status_icons::StatusIcon> {
        let mut out = Vec::new();
        for (&e, site) in &town.construction.sites {
            let t = &town.construction.templates[site.template];
            let Some(d) = &t.status_icon else { continue };
            let Some(sheet) = d.sheet.clone() else {
                continue;
            };
            let mut mask = town
                .construction
                .workforce
                .workplaces
                .get(&e)
                .map_or(0, |w| w.status);
            if let Some(h) = town.construction.homes.get(&e) {
                mask |= h.status;
            }
            if mask == 0 || !town.placed[e].alive {
                continue;
            }
            let p = &town.placed[e];
            let anchor = match t.icon_height.filter(|_| site.parts.is_empty()) {
                Some(h) => glam::Vec3::from(p.position) + glam::Vec3::Z * h,
                None => {
                    let (x, y) = p.cell;
                    let (w, h) = p.footprint;
                    let n = town.cells.size as i32;
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
