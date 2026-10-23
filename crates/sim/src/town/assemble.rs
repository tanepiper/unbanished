//! Assembling a new town from its parts, and building its starting structures.

use super::*;

impl StartingTown {
    /// The setup New Game and loading share: the citizen description's rules, models and
    /// tools, the professions, the name databases and the agents.
    pub(super) fn assemble(archive: &Archive, citizen: &str, parts: TownParts) -> StartingTown {
        let TownParts {
            start,
            citizens,
            rules,
            materials,
            storages,
            storage_sites,
            templates,
            placed,
            cells,
            scheduler,
            removed,
            animation_events,
            wildlife,
            known,
            known_materials,
            water,
        } = parts;
        let citizen_model = read(archive, r"Template\Citizen.rsc:model")
            .and_then(|d| unbanished_formats::game::model_description(&d).ok());
        let citizen_ref = normalise(citizen);
        let character = read(archive, &citizen_ref.replace(":resource", ":character"))
            .and_then(|d| unbanished_formats::game::character_description(&d).ok())
            .unwrap_or_default();
        let ai_rules = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
            .map(|c| CitizenRulesAi {
                thresholds: Thresholds {
                    adult_age: c.f(0x44),
                    health_low: c.f(0x74),
                    happy_low: c.f(0xb4),
                    happy_unhappy: c.f(0xb8),
                    hungry: c.f(0xc8),
                    hungry_urgent: c.f(0xcc),
                    cold: c.f(0xe8),
                    cold_urgent: c.f(0xec),
                },
                health: c.f(0x6c),
                happiness: c.f(0xa4),
                loiter_range: c.i(0xbc),
                clothing_duration: c.f(0x130) * 300.0,
                tool_grade: c.i(0x110),
                work_near: c.i(0xfc),
                work_far: c.i(0x100),
                work_minutes: c.i(0x104),
                work_ok: c.i(0x108),
                work_cold: c.f(0x10c),
                no_tool_factor: c.f(0x114),
                education: c.i(0x118),
                water_distance: c.i(0xf8),
                health_max: c.f(0x70),
                hunger_limit: c.f(0xd0),
                cold_limit: c.f(0xf0),
                profession_allows_idle: crate::mods::profession_list(archive)
                    .first()
                    .cloned()
                    .and_then(|r| read(archive, &r))
                    .and_then(|d| unbanished_formats::game::profession(&d).ok())
                    .is_some_and(|p| p.flag_4c),
            })
            .unwrap_or_default();
        let carried = read(archive, &citizen_ref.replace(":resource", ":storage"))
            .and_then(|d| storage_description(&d).ok())
            .map(|d| Storage {
                capacity: d.capacity,
                accepts: d.accepts,
                items: Vec::new(),
            })
            .unwrap_or_default();
        let mut agents = Agents::new(&citizens, &cells, character, ai_rules, carried, |c| {
            if c.is_adult(&rules) {
                1.0
            } else {
                CHILD_SCALE
            }
        });
        // The citizens already in the characters' tree keep their slots.
        for i in 0..citizens.len() {
            if let Some(s) = wildlife
                .tree
                .slot_of(unbanished_game::behaviour::character_tree::Who::Citizen(i))
            {
                agents.set_tree_slot(i, s);
            }
        }
        if let Some(d) = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
        {
            agents.life = unbanished_game::people::life::LifeRules::new(&d);
        }
        agents.diseases = read(archive, "Game/Disease/Diseases.rsc:resource")
            .and_then(|d| unbanished_formats::game::external_list(&d).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| read(archive, &normalise(r)))
            .filter_map(|d| unbanished_formats::game::disease(&d).ok())
            .collect();
        agents.carry = read(archive, "Game/Profession/Profession.rsc:laborer")
            .and_then(|d| unbanished_formats::game::profession(&d).ok())
            .map_or(0, |p| p.value_50);
        let names = ["Male", "Female"].map(|n| {
            archive
                .read(&format!("Game/Names/{n}_resource.crs"))
                .ok()
                .and_then(|d| NameDatabase::read(&d).ok())
        });
        let names = match names {
            [Some(m), Some(f)] => Some((m, f)),
            _ => None,
        };
        let archive_professions = ["laborer", "child", "student"]
            .iter()
            .filter_map(|k| {
                read(archive, &format!("Game/Profession/Profession.rsc:{k}"))
                    .and_then(|d| unbanished_formats::game::profession(&d).ok())
                    .map(|p| (k.to_string(), p))
            })
            .collect();
        let mut town = StartingTown {
            citizen: citizen.to_string(),
            archive_professions,
            start,
            citizens,
            rules,
            materials,
            storages,
            storage_sites,
            templates,
            placed,
            cells,
            scheduler,
            visits: Vec::new(),
            citizen_handles: Vec::new(),
            ambience_handles: Vec::new(),
            removed,
            animation_events,
            citizen_anims: citizen_model
                .as_ref()
                .map(|m| citizen_animations(archive, m))
                .unwrap_or_default(),
            citizen_model,
            nomad_materials: HashMap::new(),
            agents,
            wildlife,
            ghost: None,
            construction: Construction {
                professions: professions(archive),
                known,
                known_materials,
                water,
                names: names.clone().map(std::sync::Arc::new),
                ..Default::default()
            },
            names,
            display_cells: BTreeMap::new(),
            save_ids: None,
        };
        town.resolve_piles(archive);
        town.schedule_citizens();
        // The citizens' clips name their sound events through CitizenAnims, outside any template.
        let groups: Vec<_> = town.citizen_anims.iter().flatten().cloned().collect();
        super::templates::anim_audio_events(
            archive,
            groups.iter().map(|g| &**g),
            &mut town.agents.sounds.events,
        );
        // CitizenDescription `+0x20/+0x28/+0x30`: the teacher's, priest's and doctor's
        // professions.
        if let Some(d) = read(archive, &citizen_ref.replace(":resource", ":citizen"))
            .and_then(|d| citizen_description(&d).ok())
        {
            let list = &town.construction.professions.list;
            town.agents.life.services = d.refs.clone().map(|r| {
                let r = normalise(&r?);
                list.iter().position(|p| normalise(&p.reference) == r)
            });
        }
        town
    }

    /// The start structures (`FUN_1400a7490`) are built at once: the original creates them
    /// with `Game +0xd0` bit 0 set, so their ClearArea clears at once (`FUN_14011d230`), the
    /// BuildComponent skips gathering (`FUN_14010d760`) and finishes on entering its build
    /// state (`FUN_14010cfb0`). Each becomes a site that enters state 2 on its activation, its
    /// map stamped and its zone taken, keeping the storage its starting goods went to.
    pub(super) fn build_structures(&mut self, archive: &Archive, count: usize) {
        for e in 0..count {
            if !self.placed[e].alive {
                continue;
            }
            let before = self.storages.len();
            self.add_site(archive, e);
            let Some(site) = self.construction.sites.get_mut(&e) else {
                continue;
            };
            if let Some(s) = self.storage_sites.iter().find(|s| s.entity == e) {
                if site.storage == before && self.storages.len() == before + 1 {
                    self.storages.pop();
                }
                site.storage = s.storage;
            }
            let t = &self.construction.templates[site.template];
            site.stages = 1;
            site.progress = t.work;
            site.complete = true;
            // Activation runs the machine from state 2.
            site.machine.pending = 2;
        }
    }
}

/// A start structure's footprint and zone on the map. The start structures are built at once
/// (`Game |= 1`), so they hold their cells from the moment they are placed: the deer population's
/// capacity (`FUN_1400544b0`), which counts a cell with an occupant as forbidden, already leaves
/// out their population cells (*measured* on `Save2`, where herd 10's target is only reachable
/// with the four population cells under the barn and the house left out).
///
/// Original: `FUN_1400a7490`, `FUN_14010cfb0`.
pub(super) fn stamp_structure(cells: &mut Cells, templates: &[Template], p: &Placed, e: usize) {
    let Some(map) = templates[p.template].map.clone() else {
        return;
    };
    let rect = unbanished_game::world::placement::Rect {
        x0: p.cell.0,
        y0: p.cell.1,
        x1: p.cell.0 + p.footprint.0 - 1,
        y1: p.cell.1 + p.footprint.1 - 1,
    };
    let degrees = p.yaw as i32;
    let rules = unbanished_game::world::placement::PlacementRules {
        mask: 0,
        masks: Vec::new(),
        mask_chars: String::new(),
        footprint: map.footprint.clone(),
        width: 0,
        height: 0,
        cell_type: map.cell_type as u8,
    };
    if !map.no_stamp {
        unbanished_game::world::placement::stamp(cells, &rules, rect, degrees, e);
    }
    unbanished_game::world::placement::zone(cells, &rules, rect, degrees, e);
}
