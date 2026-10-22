//! The building state callbacks for building a site and moving into a residence.

use super::*;

impl Sim<'_, '_> {
    /// The [`Callbacks::call`] arms for building and residences.
    pub(super) fn call_building(&mut self, jobs: &mut Jobs, e: usize, f: Callback) -> bool {
        match f {
            Callback::BuildEnter1 => {
                // `BuildComponent::EnterGather`: the MultiObject's stage count, else 1; the
                // ClearArea stamps the terrain.
                let site = self.sites.get_mut(&e).unwrap();
                site.stages = site.multi_stages.unwrap_or(1);
                self.clear_area_stamp(e);
                if !self.site_list.contains(&e) {
                    self.site_list.push(e);
                }
                if !self.has_materials(e) {
                    let site = &self.sites[&e];
                    let t = &self.site_templates[site.template];
                    let stored: Vec<i32> = t
                        .materials
                        .iter()
                        .map(|&(_, m)| self.storages[site.storage].available(m))
                        .collect();
                    let gather = Gather {
                        transit: vec![0; stored.len()],
                        stored,
                        workers: Default::default(),
                    };
                    let work = Work::new(
                        e,
                        self.rect_cell(e),
                        self.professions.laborer,
                        WorkKind::Gather(gather),
                    );
                    self.set_work(jobs, e, work, CONSTRUCTION_MASK, true);
                    let site = &self.sites[&e];
                    let t = &self.site_templates[site.template];
                    let (mut capacity, mut accepts) = (0i32, 0u32);
                    for &(n, m) in &t.materials {
                        accepts |= self.materials[m].flags;
                        capacity += n * self.materials[m].size * site.stages;
                    }
                    let s = &mut self.storages[site.storage];
                    s.items.clear();
                    s.capacity = capacity;
                    s.accepts = accepts;
                } else {
                    self.machine(e).request(8);
                }
                true
            }
            Callback::BuildUpdate1 => {
                if self.has_materials(e) {
                    self.remove_work(jobs, e);
                    self.machine(e).request(8);
                }
                true
            }
            Callback::BuildEnter8 => {
                self.decal_opaque(e);
                self.set_mesh(e, 0);
                let work = Work::new(
                    e,
                    self.rect_cell(e),
                    self.professions.builder,
                    WorkKind::Build(BuildWork {
                        occupied: vec![false; self.build_point_count(e)],
                        ..Default::default()
                    }),
                );
                self.set_work(jobs, e, work, CONSTRUCTION_MASK, true);
                true
            }
            Callback::BuildUpdate8 => {
                let site = &self.sites[&e];
                let total = self.site_templates[site.template].work * site.stages;
                let meshes = self.meshes(e);
                if site.progress < total {
                    let mut m = ((site.progress as f32 / total as f32) * meshes as f32) as i32;
                    if m == meshes - 1 {
                        m = meshes - 2;
                    }
                    self.set_mesh(e, m);
                } else {
                    self.remove_work(jobs, e);
                    self.set_mesh(e, meshes - 1);
                    self.machine(e).request(2);
                }
                true
            }
            Callback::BuildExit8 => {
                self.site_list.retain(|&s| s != e);
                // `FUN_14010cad0`: the build is no longer paused.
                self.set_icon(e, crate::work::professions::icon::BUILD_PAUSED, false);
                // Entity event 4 (`FUN_1400c8040`): the pasture's shelter points are cleared;
                // a TerraformComponent digs, the entity's then its MultiObject parts'
                // (`FUN_14012a960`).
                if self.wildlife.pastures.contains_key(&e) {
                    self.pasture_built(e);
                }
                let parts = self.sites[&e].parts.clone();
                for x in std::iter::once(e).chain(parts) {
                    if self.templates[self.placed[x].template].terraform.is_some() {
                        self.terraform_requests
                            .push(crate::world::terraform::Terraform::Dig(x));
                    }
                }
                // A QuarryComponent digs its pit to the shallow floor (`FUN_140055a50`).
                if let Some(q) = &self.templates[self.placed[e].template].quarry {
                    self.terraform_requests
                        .push(crate::world::terraform::Terraform::Quarry {
                            entity: e,
                            floor: q.shallow,
                            claim: false,
                        });
                }
                true
            }
            Callback::BuildFinish => {
                self.finish_storage(e);
                self.sites.get_mut(&e).unwrap().complete = true;
                false
            }
            Callback::ResidenceEnter2 => {
                // `FUN_14012f2b0`: register; the component's family slots come from the
                // description (it keeps its state while out of the town's).
                self.workforce.residences.push(e);
                let site = self.sites.get_mut(&e).unwrap();
                let kept = site.home.take();
                let storage = site.storage;
                if let Some(desc) = self.site_templates[site.template].residence {
                    let home =
                        kept.unwrap_or_else(|| crate::people::life::Home::new(desc, storage));
                    self.homes.entry(e).or_insert(home);
                }
                false
            }
            Callback::ResidenceEnter3 => {
                self.residence_leave(e);
                false
            }
            _ => unreachable!(),
        }
    }
}
