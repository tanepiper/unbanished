//! Starting and stopping the systems that buildings, citizens and disasters ask for.

use super::*;

impl Particles {
    pub(super) fn node(&mut self, archive: &Archive, mesh: &str, name: &str) -> Option<Vec3> {
        let nodes = self.nodes.entry(mesh.to_string()).or_insert_with(|| {
            let data = archive
                .read_ref(&crate::view::scene_assets::with_object(mesh))
                .ok()?;
            let m = unbanished_formats::types::graphics_mesh(&data).ok()?;
            let locals: Vec<_> = m
                .bones
                .iter()
                .map(unbanished_formats::skeleton::local_bind)
                .collect();
            let world = unbanished_formats::skeleton::world_transforms(&m.bones, &locals);
            Some(
                m.bones
                    .iter()
                    .zip(world)
                    .map(|(b, w)| (b.name.clone(), Vec3::from(w.translation)))
                    .collect(),
            )
        });
        nodes
            .as_ref()?
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| *p)
    }

    pub(super) fn point(&mut self, archive: &Archive, list: &str, name: &str) -> Option<Vec3> {
        let points = self.points.entry(list.to_string()).or_insert_with(|| {
            let data = archive
                .read_ref(&crate::view::scene_assets::with_object(list))
                .ok()?;
            let l = unbanished_formats::game::point_list(&data).ok()?;
            Some(
                l.points
                    .into_iter()
                    .map(|p| (p.name, Vec3::from(p.position)))
                    .collect(),
            )
        });
        points
            .as_ref()?
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| *p)
    }

    /// The entities' ParticleComponents against the town this frame:
    /// - a slot following entity events 2 and 3 (`+0x12`) runs while its building has
    ///   residents (`ResidenceComponent::AddResident`/`RemoveResident`) or workers of its own
    ///   profession (`FUN_140134ef0`/`FUN_140134cb0`);
    /// - the FireDescription's slot while the building burns (`FireComponent::Ignite`/`PutOut`);
    /// - a field's, orchard's or pasture's slot while it is blighted (DiseaseComponent).
    pub fn drive(
        &mut self,
        archive: &Archive,
        town: &crate::start::StartingTown,
        grid: &unbanished_game::terrain::HeightGrid,
    ) {
        let c = &town.construction;
        let mut seen = std::collections::BTreeSet::new();
        for (e, p) in town.placed.iter().enumerate() {
            if !p.alive {
                continue;
            }
            let template = &town.templates[p.template];
            let Some(desc) = self.description(archive, &template.reference) else {
                continue;
            };
            seen.insert(e);
            let site = c.sites.get(&e);
            let site_template = site.map(|s| &c.templates[s.template]);
            let fire_slot = site_template
                .and_then(|t| t.fire.as_ref())
                .map(|f| f.particle as usize);
            let occupied = c
                .homes
                .get(&e)
                .is_some_and(|h| h.families.iter().any(|f| !f.is_empty()))
                || c.workforce.workplaces.get(&e).is_some_and(|w| {
                    !w.workers.is_empty() && w.profession == w.description_profession
                });
            let burning = site
                .and_then(|s| s.fire.as_ref())
                .is_some_and(|f| 0 < f.burn);
            let sick = site
                .and_then(|s| s.disease.as_ref())
                .is_some_and(|b| 0 < b.left);
            // A tornado's system follows it (`FUN_140058630`) and fades out once it reaches
            // the end of its line (`FUN_140054080(0)`).
            let tornado = c.tornadoes.get(&e);
            for (k, slot) in desc.slots.iter().enumerate() {
                let key = (e, k);
                if let Some(t) = tornado {
                    if let Some(s) = self.slots.get_mut(&key) {
                        s.instance.position = Vec3::from(t.position);
                    }
                }
                let want = if let Some(t) = tornado.filter(|_| k == 0) {
                    if t.emitting {
                        Want::On
                    } else {
                        Want::Off
                    }
                } else if slot.flags[1] {
                    Want::On
                } else if slot.flags[2] {
                    if occupied {
                        Want::On
                    } else {
                        Want::Off
                    }
                } else if fire_slot == Some(k) {
                    if burning {
                        Want::On
                    } else {
                        Want::Off
                    }
                } else if sick {
                    // The blight's end (`FUN_140048c80`) fades every slot of the component out
                    // (`FUN_140054150`, `-1 / time` each); a field's only slot is this one.
                    Want::On
                } else {
                    Want::Off
                };
                if want == Want::Off || self.is_running(key) {
                    self.set(key, want, None);
                    continue;
                }
                let Some(system) = slot.system.as_deref().and_then(|r| self.system(archive, r))
                else {
                    continue;
                };
                // Where: the map rectangle's middle on the ground (flag 3), the points
                // model's point, the model node, else the model's root.
                let matrix = glam::Mat4::from_cols_array(&crate::gfx::models::placement_matrix(
                    p.position,
                    [p.scale[0], p.scale[1], p.scale[2]],
                    p.yaw,
                ));
                let local = if slot.flags[3] || template.model.is_none() {
                    None
                } else if let Some(list) = &desc.extra {
                    self.point(archive, list, &slot.node)
                } else {
                    let mesh = template
                        .model
                        .as_ref()
                        .and_then(|m| m.variants.get(p.variant))
                        .and_then(|v| v.meshes.get(p.mesh))
                        .cloned()
                        .flatten();
                    mesh.and_then(|m| self.node(archive, &m, &slot.node))
                };
                let position = if let Some(t) = tornado {
                    Vec3::from(t.position)
                } else if slot.flags[3] || template.model.is_none() {
                    let n = town.cells.size as i32;
                    let (x, y) = p.cell;
                    let (w, h) = p.footprint;
                    let cell = (y + (h >> 1)) * n + (w >> 1) + x;
                    Vec3::from(unbanished_game::town::cell_ground(
                        town.cells.size,
                        grid,
                        cell,
                    ))
                } else {
                    matrix.transform_point3(local.unwrap_or(Vec3::ZERO))
                };
                self.set(
                    key,
                    Want::On,
                    Some((
                        system,
                        position,
                        slot.fade as f32 * 1e-6,
                        slot.prewarm as f32 * 1e-6,
                    )),
                );
            }
        }
        self.retain(|e| seen.contains(&e));
    }
}
