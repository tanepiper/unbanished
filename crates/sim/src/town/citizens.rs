//! The starting citizens: households, goods, animations, tools, rules and their schedule.

use super::*;

/// `Template\X.rsc:resource` (profile lists omit the object name).
/// `DAT_140178818`: children's model scale.
pub(crate) const CHILD_SCALE: f32 = 0.66;

/// `FUN_1400a87d0` → `FUN_140097330` / `FUN_1400a73d0`: the profile's raw materials become the
/// candidate list, then seven `(mask, cap)` passes of `FUN_140096b30` fill the town storages.
///
/// Town storage registrations retain the placed entity identity and creation order. Capacity is
/// `capacity · (per-cell ? footprint area : 1)` as in `FUN_140131b10`.
pub(super) fn starting_goods(
    archive: &Archive,
    profile: &MapGenerator,
    placed: &[Placed],
    templates: &[Template],
) -> (Vec<RawMaterial>, Vec<Storage>, Vec<StorageSite>) {
    let mut materials = Vec::new();
    for r in &profile.raw_materials {
        let key = normalise(r);
        if materials.iter().any(|m: &RawMaterial| m.reference == key) {
            continue;
        }
        let Some(d) = read(archive, &key.replace(":resource", ":rawmaterial"))
            .and_then(|d| raw_material_description(&d).ok())
        else {
            continue;
        };
        materials.push(RawMaterial {
            reference: key,
            size: d.size,
            flags: d.flags,
            tool_uses: d.value_54,
            preferred: d.flag_58,
            pile_spacing: d.value_4c,
            pile_amount: d.value_40,
        });
    }
    let mut storages = Vec::new();
    let mut sites = Vec::new();
    for (entity, p) in placed.iter().enumerate() {
        let t = &templates[p.template];
        if !t.storage_location && !t.market_location {
            continue;
        }
        let Some((accepts, capacity, per_cell)) = t.storage else {
            continue;
        };
        let (w, h) = p.footprint;
        sites.push(StorageSite {
            entity,
            storage: storages.len(),
            location_flags: t.storage_location_flags,
            radius: if t.market_location {
                t.market_radius
            } else {
                None
            },
        });
        storages.push(Storage {
            capacity: capacity * if per_cell { w * h } else { 1 },
            accepts,
            items: Vec::new(),
        });
    }
    // FUN_140096b30 walks the ordinary storage registry, excluding market owners.
    let mut ordinary: Vec<_> = sites
        .iter()
        .filter(|s| s.radius.is_none())
        .map(|s| storages[s.storage].clone())
        .collect();
    let candidates: Vec<usize> = (0..materials.len()).collect();
    const MASKS: [u32; 7] = [0x400, 0x800, 0x1000, 0x200, 0x20, 0x40, 0x4000];
    for (i, mask) in MASKS.iter().enumerate() {
        distribute(
            &mut ordinary,
            &materials,
            &candidates,
            *mask,
            profile.ints[i + 1],
        );
    }
    for (site, inventory) in sites.iter().filter(|s| s.radius.is_none()).zip(ordinary) {
        storages[site.storage] = inventory;
    }
    for s in &storages {
        log::debug!(
            "storage cap {} accepts {:#x} used {}",
            s.capacity,
            s.accepts,
            s.used(&materials)
        );
    }
    (materials, storages, sites)
}

/// Each citizen model variant's animation group.
pub(super) fn citizen_animations(
    archive: &Archive,
    model: &unbanished_formats::game::ModelDescription,
) -> Vec<Option<Arc<unbanished_formats::types::AnimationGroup>>> {
    model
        .variants
        .iter()
        .map(|v| {
            v.animations
                .as_deref()
                .and_then(|r| read(archive, r))
                .and_then(|d| unbanished_formats::types::animation_group(&d).ok())
                .map(Arc::new)
        })
        .collect()
}

/// Each citizen variant's mesh nodes and the index of its `tool` node.
pub(crate) fn citizen_tool_nodes(
    archive: &Archive,
    model: &unbanished_formats::game::ModelDescription,
) -> Vec<Option<(Vec<unbanished_formats::types::Bone>, usize)>> {
    model
        .variants
        .iter()
        .map(|v| {
            let mesh = v.meshes.first().cloned().flatten()?;
            let m = unbanished_formats::types::graphics_mesh(&read(archive, &mesh)?).ok()?;
            let tool = m.bones.iter().position(|b| b.name == "tool")?;
            Some((m.bones, tool))
        })
        .collect()
}

/// `FUN_140002080`: the start cell's centre on the ground.
pub(super) fn grid_start(map_size: usize, terrain: &HeightGrid, start: (i32, i32)) -> [f32; 3] {
    let n = map_size as i32;
    let c = (n >> 1) as f32 - 0.5;
    let (x, y) = (start.0 as f32 - c, start.1 as f32 - c);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

/// The CitizenDescription's ages and education (`citizen` is the profile's citizen template).
pub(super) fn citizen_rules(archive: &Archive, citizen: &str) -> Option<CitizenRules> {
    read(
        archive,
        &normalise(citizen).replace(":resource", ":citizen"),
    )
    .and_then(|d| citizen_description(&d).ok())
    .map(|c| CitizenRules {
        adult_age: c.adult_age(),
        start_age: c.start_age(),
        lifespan: c.lifespan(),
        education: c.i(0x118),
    })
}

pub(super) fn households(
    archive: &Archive,
    profile: &MapGenerator,
    start: &[f32; 3],
    rng: &mut Random,
) -> (Vec<Citizen>, CitizenRules) {
    let rules = citizen_rules(archive, &profile.citizen);
    let names = |n: &str| {
        archive
            .read(&format!("Game/Names/{n}_resource.crs"))
            .ok()
            .and_then(|d| NameDatabase::read(&d).ok())
    };
    let (Some(rules), Some(male), Some(female)) = (rules, names("Male"), names("Female")) else {
        log::warn!("households: citizen description or name databases missing");
        return (
            Vec::new(),
            CitizenRules {
                adult_age: 2.0,
                start_age: (2.2, 8.0),
                lifespan: (15.0, 3.0),
                education: 0,
            },
        );
    };
    let mut h = Households {
        rules,
        male_names: &male,
        female_names: &female,
        spread: profile.value_40,
        start: *start,
        citizens: Vec::new(),
    };
    h.create_all(rng, &profile.households);
    log::info!(
        "{} citizens: {:?}",
        h.citizens.len(),
        h.citizens
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
    );
    (h.citizens, rules)
}

impl StartingTown {
    /// The citizens' entities in the scheduler (the citizen template's bucket 1, `FUN_1400c9d70`):
    /// the living ones not yet registered join, the dead leave (`FUN_1400c8db0`).
    pub(super) fn schedule_citizens(&mut self) {
        /// `Template\Citizen.rsc`'s bucket.
        const CITIZEN_BUCKET: usize = 1;
        let extra = unbanished_game::scheduler::EXTRA;
        self.citizen_handles.resize(self.citizens.len(), None);
        for (c, citizen) in self.citizens.iter().enumerate() {
            match (citizen.alive, self.citizen_handles[c]) {
                (true, None) => {
                    self.citizen_handles[c] =
                        Some(self.scheduler.register_extra(extra + c, CITIZEN_BUCKET));
                }
                (false, Some(h)) => {
                    self.scheduler.remove(h);
                    self.citizen_handles[c] = None;
                }
                _ => {}
            }
        }
    }

    /// The citizens' animation, characters and AI for one simulation iteration
    /// (`unbanished_game::behaviour::agents`). `stock` is the stock manager's `(count, limit)` per slot.
    /// Returns whether any citizen is drawn.
    pub fn update_citizens(
        &mut self,
        terrain: &HeightGrid,
        rng: &mut Random,
        dt: f32,
        temperature: f32,
        stock: [(i32, i32); 21],
    ) -> bool {
        let before = self.placed.len();
        let groups: Vec<_> = self
            .citizens
            .iter()
            .map(|c| {
                self.citizen_anims
                    .get(usize::from(!c.male))
                    .and_then(Option::as_deref)
                    .filter(|_| c.alive)
            })
            .collect();
        let mut env = unbanished_game::town::Env {
            wildlife: &mut self.wildlife,
            cells: &mut self.cells,
            placed: &mut self.placed,
            templates: &self.templates,
            terrain,
            materials: &self.materials,
            storages: &mut self.storages,
            sites: &self.storage_sites,
            rng,
            construction: &mut self.construction,
            stock,
            temperature,
            visits: &self.visits,
        };
        let rules = self.rules;
        let names = self.names.as_ref();
        let mut newborn = |rng: &mut Random, mother: &Citizen, male: bool| match names {
            Some((m, f)) => {
                unbanished_game::people::citizens::newborn(rng, &rules, m, f, mother, male)
            }
            None => Citizen {
                male,
                alive: true,
                ..mother.clone()
            },
        };
        self.agents
            .update(&mut self.citizens, &mut env, &groups, dt, &mut newborn);
        self.visits.clear();
        self.schedule_citizens();
        // `FUN_1400c9d70`: piles left by cleared resources join the entity manager's buckets.
        for id in before..self.placed.len() {
            self.register_entity(id);
        }
        self.apply_rebuilds();
        self.apply_storage_changes();
        self.place_displays(terrain);
        !self.citizens.is_empty()
    }

    /// StockManager::Update's commute optimisation (`unbanished_game::behaviour::commute`) by `dt` seconds.
    pub fn update_commute(&mut self, terrain: &HeightGrid, rng: &mut Random, dt: f32) {
        let groups: Vec<_> = self
            .citizens
            .iter()
            .map(|c| {
                self.citizen_anims
                    .get(usize::from(!c.male))
                    .and_then(Option::as_deref)
                    .filter(|_| c.alive)
            })
            .collect();
        let mut env = unbanished_game::town::Env {
            wildlife: &mut self.wildlife,
            cells: &mut self.cells,
            placed: &mut self.placed,
            templates: &self.templates,
            terrain,
            materials: &self.materials,
            storages: &mut self.storages,
            sites: &self.storage_sites,
            rng,
            construction: &mut self.construction,
            stock: [(0, 0); 21],
            temperature: 0.0,
            visits: &[],
        };
        self.agents.update_commute(
            &self.citizens,
            &mut env,
            &groups,
            unbanished_game::scheduler::ticks(dt),
        );
    }
}
